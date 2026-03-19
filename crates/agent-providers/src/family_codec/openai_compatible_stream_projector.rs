use agent_core::{CanonicalStreamEvent, FinishReason, ProviderRawStreamEvent, Usage};

use crate::error::AdapterError;
use crate::interfaces::ProviderStreamProjector;
use crate::openai_family::streaming::{project_output_item_added, project_output_item_done};
use crate::openai_family::types::OpenAiResponsesBody;

#[derive(Debug, Default)]
pub(crate) struct OpenAiStreamProjector {
    response_started: bool,
    completed: bool,
}

impl ProviderStreamProjector for OpenAiStreamProjector {
    fn project(
        &mut self,
        raw: ProviderRawStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, AdapterError> {
        let Some(value) = raw.json() else {
            return Ok(Vec::new());
        };

        let event_type = json_str(value, "type");
        let mut events = Vec::new();

        match event_type {
            Some("response.created") | Some("response.in_progress") => {
                if !self.response_started {
                    self.response_started = true;
                    let response = parse_responses_body(value.get("response"));
                    events.push(CanonicalStreamEvent::ResponseStarted {
                        model: response
                            .as_ref()
                            .and_then(|response| response.model.clone()),
                        response_id: value
                            .get("response")
                            .and_then(|response| json_str(response, "id"))
                            .map(ToOwned::to_owned),
                    });
                }
            }
            Some("response.output_item.added") => {
                if let Some(event) = project_output_item_added(value) {
                    events.push(event);
                }
            }
            Some("response.output_text.delta") => {
                if let Some(delta) = json_str(value, "delta") {
                    events.push(CanonicalStreamEvent::TextDelta {
                        output_index: json_u32(value, "output_index").unwrap_or(0),
                        content_index: json_u32(value, "content_index").unwrap_or(0),
                        item_id: json_str(value, "item_id").map(ToOwned::to_owned),
                        delta: delta.to_string(),
                    });
                }
            }
            Some("response.function_call_arguments.delta") => {
                if let Some(delta) = json_str(value, "delta") {
                    let output_index = json_u32(value, "output_index").unwrap_or(0);
                    events.push(CanonicalStreamEvent::ToolCallArgumentsDelta {
                        output_index,
                        tool_call_index: output_index,
                        item_id: json_str(value, "item_id").map(ToOwned::to_owned),
                        tool_call_id: None,
                        tool_name: None,
                        delta: delta.to_string(),
                    });
                }
            }
            Some("response.output_item.done") => {
                if let Some(event) = project_output_item_done(value) {
                    events.push(event);
                }
            }
            Some("response.completed") => {
                if let Some(response) = value.get("response") {
                    let parsed = parse_responses_body(Some(response));

                    if let Some(message) = parsed.as_ref().and_then(parse_error_message) {
                        events.push(CanonicalStreamEvent::Failed { message });
                        self.completed = true;
                        return Ok(events);
                    }

                    if let Some(usage) = response
                        .get("usage")
                        .and_then(parse_openai_usage)
                        .or_else(|| parsed.as_ref().and_then(parse_usage_from_body))
                    {
                        events.push(CanonicalStreamEvent::UsageUpdated { usage });
                    }

                    if !self.completed {
                        self.completed = true;
                        events.push(CanonicalStreamEvent::Completed {
                            finish_reason: parsed
                                .as_ref()
                                .map(infer_openai_finish_reason_from_body)
                                .unwrap_or_else(|| infer_openai_finish_reason(response)),
                        });
                    }
                }
            }
            Some("error") => {
                if let Some(message) = value
                    .get("error")
                    .and_then(|error| json_str(error, "message"))
                    .or_else(|| json_str(value, "message"))
                {
                    self.completed = true;
                    events.push(CanonicalStreamEvent::Failed {
                        message: message.to_string(),
                    });
                }
            }
            _ => {}
        }

        Ok(events)
    }
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn json_u32(value: &serde_json::Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn parse_openai_usage(value: &serde_json::Value) -> Option<Usage> {
    Some(Usage {
        input_tokens: value
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64),
        output_tokens: value
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64),
        cached_input_tokens: value
            .get("input_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(serde_json::Value::as_u64),
        total_tokens: value
            .get("total_tokens")
            .and_then(serde_json::Value::as_u64),
    })
}

fn parse_responses_body(value: Option<&serde_json::Value>) -> Option<OpenAiResponsesBody> {
    serde_json::from_value(value?.clone()).ok()
}

fn parse_error_message(response: &OpenAiResponsesBody) -> Option<String> {
    response
        .error
        .as_ref()
        .and_then(|error| error.message.as_ref())
        .and_then(value_to_string)
}

fn parse_usage_from_body(response: &OpenAiResponsesBody) -> Option<Usage> {
    response.usage.clone().map(Usage::from)
}

fn infer_openai_finish_reason_from_body(response: &OpenAiResponsesBody) -> FinishReason {
    let has_function_call = response
        .output
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .any(|item| json_str(item, "type") == Some("function_call"))
        })
        .unwrap_or(false);

    if has_function_call {
        FinishReason::ToolCalls
    } else {
        FinishReason::Stop
    }
}

fn infer_openai_finish_reason(response: &serde_json::Value) -> FinishReason {
    let has_function_call = response
        .get("output")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .any(|item| json_str(item, "type") == Some("function_call"))
        })
        .unwrap_or(false);

    if has_function_call {
        FinishReason::ToolCalls
    } else {
        FinishReason::Stop
    }
}

fn value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}
