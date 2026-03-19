use agent_core::{
    CanonicalStreamEvent, FinishReason, ProviderRawStreamEvent, RawStreamPayload, Usage,
};

use crate::{
    error::{AdapterError, AdapterErrorKind, AdapterOperation},
    families::openai_compatible::wire::{
        streaming::{project_output_item_added, project_output_item_done},
        types::OpenAiResponsesBody,
    },
    interfaces::ProviderStreamProjector,
};

#[derive(Debug, Default)]
pub(crate) struct OpenRouterStreamProjector {
    response_started: bool,
    completed: bool,
}

impl ProviderStreamProjector for OpenRouterStreamProjector {
    fn project(
        &mut self,
        raw: ProviderRawStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, AdapterError> {
        match &raw.payload {
            RawStreamPayload::Empty | RawStreamPayload::Text { .. } => Ok(Vec::new()),
            RawStreamPayload::Done => {
                if self.completed {
                    return Ok(Vec::new());
                }
                self.completed = true;
                Ok(vec![CanonicalStreamEvent::Completed {
                    finish_reason: FinishReason::Other,
                }])
            }
            RawStreamPayload::Json { value } => {
                if json_str(value, "type").is_none() {
                    return Err(AdapterError::new(
                        AdapterErrorKind::ProtocolViolation,
                        agent_core::ProviderKind::OpenRouter,
                        AdapterOperation::ProjectStreamEvent,
                        "OpenRouter streaming expected Responses SSE payload with top-level 'type'",
                    ));
                }

                Ok(self.project_responses_event(value))
            }
        }
    }
}

impl OpenRouterStreamProjector {
    fn project_responses_event(&mut self, value: &serde_json::Value) -> Vec<CanonicalStreamEvent> {
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
                        return events;
                    }

                    if let Some(usage) = response
                        .get("usage")
                        .and_then(parse_openrouter_usage)
                        .or_else(|| parsed.as_ref().and_then(parse_usage_from_body))
                    {
                        events.push(CanonicalStreamEvent::UsageUpdated { usage });
                    }

                    if !self.completed {
                        self.completed = true;
                        events.push(CanonicalStreamEvent::Completed {
                            finish_reason: parsed
                                .as_ref()
                                .map(infer_responses_finish_reason_from_body)
                                .unwrap_or_else(|| infer_responses_finish_reason(response)),
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

        events
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

fn parse_openrouter_usage(value: &serde_json::Value) -> Option<Usage> {
    Some(Usage {
        input_tokens: value
            .get("prompt_tokens")
            .or_else(|| value.get("input_tokens"))
            .and_then(serde_json::Value::as_u64),
        output_tokens: value
            .get("completion_tokens")
            .or_else(|| value.get("output_tokens"))
            .and_then(serde_json::Value::as_u64),
        cached_input_tokens: value
            .get("prompt_tokens_details")
            .or_else(|| value.get("input_tokens_details"))
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

fn infer_responses_finish_reason_from_body(response: &OpenAiResponsesBody) -> FinishReason {
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

fn infer_responses_finish_reason(response: &serde_json::Value) -> FinishReason {
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
