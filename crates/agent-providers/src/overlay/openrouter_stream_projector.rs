use agent_core::{
    CanonicalStreamEvent, FinishReason, MessageRole, ProviderRawStreamEvent, RawStreamPayload,
    StreamOutputItemEnd, StreamOutputItemStart, Usage,
};

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::streaming::ProviderStreamProjector;

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
                    events.push(CanonicalStreamEvent::ResponseStarted {
                        model: value
                            .get("response")
                            .and_then(|response| json_str(response, "model"))
                            .map(ToOwned::to_owned),
                        response_id: value
                            .get("response")
                            .and_then(|response| json_str(response, "id"))
                            .map(ToOwned::to_owned),
                    });
                }
            }
            Some("response.output_item.added") => {
                if let Some(output_index) = json_u32(value, "output_index")
                    && let Some(item) = value.get("item")
                {
                    match json_str(item, "type") {
                        Some("message") => events.push(CanonicalStreamEvent::OutputItemStarted {
                            output_index,
                            item: StreamOutputItemStart::Message {
                                item_id: json_str(item, "id").map(ToOwned::to_owned),
                                role: parse_message_role(json_str(item, "role"))
                                    .unwrap_or(MessageRole::Assistant),
                            },
                        }),
                        Some("function_call") => {
                            if let Some(name) = json_str(item, "name") {
                                events.push(CanonicalStreamEvent::OutputItemStarted {
                                    output_index,
                                    item: StreamOutputItemStart::ToolCall {
                                        item_id: json_str(item, "id").map(ToOwned::to_owned),
                                        tool_call_id: json_str(item, "call_id")
                                            .map(ToOwned::to_owned),
                                        name: name.to_string(),
                                    },
                                });
                            }
                        }
                        _ => {}
                    }
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
                if let Some(output_index) = json_u32(value, "output_index")
                    && let Some(item) = value.get("item")
                {
                    match json_str(item, "type") {
                        Some("message") => {
                            events.push(CanonicalStreamEvent::OutputItemCompleted {
                                output_index,
                                item: StreamOutputItemEnd::Message {
                                    item_id: json_str(item, "id").map(ToOwned::to_owned),
                                },
                            });
                        }
                        Some("function_call") => {
                            if let Some(name) = json_str(item, "name") {
                                events.push(CanonicalStreamEvent::OutputItemCompleted {
                                    output_index,
                                    item: StreamOutputItemEnd::ToolCall {
                                        item_id: json_str(item, "id").map(ToOwned::to_owned),
                                        tool_call_id: json_str(item, "call_id")
                                            .map(ToOwned::to_owned),
                                        name: name.to_string(),
                                        arguments_json_text: json_str(item, "arguments")
                                            .unwrap_or("")
                                            .to_string(),
                                    },
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("response.completed") => {
                if let Some(response) = value.get("response") {
                    if let Some(message) = response
                        .get("error")
                        .and_then(|error| json_str(error, "message"))
                    {
                        events.push(CanonicalStreamEvent::Failed {
                            message: message.to_string(),
                        });
                        self.completed = true;
                        return events;
                    }

                    if let Some(usage) = response.get("usage").and_then(parse_openrouter_usage) {
                        events.push(CanonicalStreamEvent::UsageUpdated { usage });
                    }

                    if !self.completed {
                        self.completed = true;
                        events.push(CanonicalStreamEvent::Completed {
                            finish_reason: infer_responses_finish_reason(response),
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

fn parse_message_role(role: Option<&str>) -> Option<MessageRole> {
    match role {
        Some("system") => Some(MessageRole::System),
        Some("user") => Some(MessageRole::User),
        Some("assistant") => Some(MessageRole::Assistant),
        Some("tool") => Some(MessageRole::Tool),
        _ => None,
    }
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
