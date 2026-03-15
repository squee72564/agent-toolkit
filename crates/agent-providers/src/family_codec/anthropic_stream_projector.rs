use std::collections::BTreeMap;

use agent_core::{
    CanonicalStreamEvent, FinishReason, MessageRole, ProviderRawStreamEvent, StreamOutputItemEnd,
    StreamOutputItemStart, Usage,
};

use crate::error::AdapterError;
use crate::stream_projector::ProviderStreamProjector;

#[derive(Debug, Clone)]
enum AnthropicBlockState {
    Text {
        item_id: Option<String>,
    },
    ToolCall {
        item_id: Option<String>,
        tool_call_id: Option<String>,
        name: String,
        arguments: String,
    },
}

impl AnthropicBlockState {
    fn item_id(&self) -> Option<String> {
        match self {
            Self::Text { item_id } => item_id.clone(),
            Self::ToolCall { item_id, .. } => item_id.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AnthropicStreamProjector {
    response_started: bool,
    completed: bool,
    stop_reason: Option<FinishReason>,
    blocks: BTreeMap<u32, AnthropicBlockState>,
}

impl ProviderStreamProjector for AnthropicStreamProjector {
    fn project(
        &mut self,
        raw: ProviderRawStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, AdapterError> {
        let Some(value) = raw.json() else {
            return Ok(Vec::new());
        };

        let mut events = Vec::new();

        match raw.sse_event_name() {
            Some("message_start") => {
                if !self.response_started {
                    self.response_started = true;
                    let message = value.get("message");
                    events.push(CanonicalStreamEvent::ResponseStarted {
                        model: message
                            .and_then(|message| json_str(message, "model"))
                            .map(ToOwned::to_owned),
                        response_id: message
                            .and_then(|message| json_str(message, "id"))
                            .map(ToOwned::to_owned),
                    });
                }
            }
            Some("content_block_start") => {
                if let Some(index) = json_u32(value, "index")
                    && let Some(block) = value.get("content_block")
                {
                    match json_str(block, "type") {
                        Some("text") => {
                            self.blocks
                                .insert(index, AnthropicBlockState::Text { item_id: None });
                            events.push(CanonicalStreamEvent::OutputItemStarted {
                                output_index: index,
                                item: StreamOutputItemStart::Message {
                                    item_id: None,
                                    role: MessageRole::Assistant,
                                },
                            });
                        }
                        Some("tool_use") => {
                            let name = json_str(block, "name").unwrap_or("").to_string();
                            let tool_call_id = json_str(block, "id").map(ToOwned::to_owned);
                            self.blocks.insert(
                                index,
                                AnthropicBlockState::ToolCall {
                                    item_id: None,
                                    tool_call_id: tool_call_id.clone(),
                                    name: name.clone(),
                                    arguments: String::new(),
                                },
                            );
                            events.push(CanonicalStreamEvent::OutputItemStarted {
                                output_index: index,
                                item: StreamOutputItemStart::ToolCall {
                                    item_id: None,
                                    tool_call_id,
                                    name,
                                },
                            });
                        }
                        _ => {}
                    }
                }
            }
            Some("content_block_delta") => {
                if let Some(index) = json_u32(value, "index")
                    && let Some(delta) = value.get("delta")
                {
                    match json_str(delta, "type") {
                        Some("text_delta") => {
                            if let Some(text) = json_str(delta, "text") {
                                events.push(CanonicalStreamEvent::TextDelta {
                                    output_index: index,
                                    content_index: 0,
                                    item_id: self
                                        .blocks
                                        .get(&index)
                                        .and_then(AnthropicBlockState::item_id),
                                    delta: text.to_string(),
                                });
                            }
                        }
                        Some("input_json_delta") => {
                            if let Some(partial_json) = json_str(delta, "partial_json")
                                && let Some(AnthropicBlockState::ToolCall {
                                    item_id,
                                    tool_call_id,
                                    name,
                                    arguments,
                                }) = self.blocks.get_mut(&index)
                            {
                                arguments.push_str(partial_json);
                                events.push(CanonicalStreamEvent::ToolCallArgumentsDelta {
                                    output_index: index,
                                    tool_call_index: index,
                                    item_id: item_id.clone(),
                                    tool_call_id: tool_call_id.clone(),
                                    tool_name: Some(name.clone()),
                                    delta: partial_json.to_string(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("content_block_stop") => {
                if let Some(index) = json_u32(value, "index")
                    && let Some(block) = self.blocks.remove(&index)
                {
                    events.push(match block {
                        AnthropicBlockState::Text { item_id } => {
                            CanonicalStreamEvent::OutputItemCompleted {
                                output_index: index,
                                item: StreamOutputItemEnd::Message { item_id },
                            }
                        }
                        AnthropicBlockState::ToolCall {
                            item_id,
                            tool_call_id,
                            name,
                            arguments,
                        } => CanonicalStreamEvent::OutputItemCompleted {
                            output_index: index,
                            item: StreamOutputItemEnd::ToolCall {
                                item_id,
                                tool_call_id,
                                name,
                                arguments_json_text: arguments,
                            },
                        },
                    });
                }
            }
            Some("message_delta") => {
                if let Some(usage) = value.get("usage").and_then(parse_anthropic_usage) {
                    events.push(CanonicalStreamEvent::UsageUpdated { usage });
                }
                if let Some(delta) = value.get("delta") {
                    self.stop_reason = parse_finish_reason(json_str(delta, "stop_reason"));
                }
            }
            Some("message_stop") => {
                if !self.completed {
                    self.completed = true;
                    events.push(CanonicalStreamEvent::Completed {
                        finish_reason: self.stop_reason.clone().unwrap_or(FinishReason::Other),
                    });
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

fn parse_finish_reason(reason: Option<&str>) -> Option<FinishReason> {
    match reason {
        Some("stop") | Some("end_turn") => Some(FinishReason::Stop),
        Some("length") | Some("max_tokens") => Some(FinishReason::Length),
        Some("tool_calls") | Some("tool_use") => Some(FinishReason::ToolCalls),
        Some("content_filter") => Some(FinishReason::ContentFilter),
        Some("error") => Some(FinishReason::Error),
        Some(_) => Some(FinishReason::Other),
        None => None,
    }
}

fn parse_anthropic_usage(value: &serde_json::Value) -> Option<Usage> {
    Some(Usage {
        input_tokens: value
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64),
        output_tokens: value
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64),
        cached_input_tokens: Some(
            value
                .get("cache_read_input_tokens")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .saturating_add(
                    value
                        .get("cache_creation_input_tokens")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                ),
        ),
        total_tokens: None,
    })
}
