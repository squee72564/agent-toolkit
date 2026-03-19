use std::collections::BTreeMap;

use agent_core::{
    CanonicalStreamEvent, FinishReason, MessageRole, ProviderRawStreamEvent, StreamOutputItemEnd,
    StreamOutputItemStart, Usage,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{
    error::AdapterError,
    families::anthropic::wire::{
        streaming::{parse_content_block_start, parse_message_delta, parse_message_start},
        types::{AnthropicTextBlock, AnthropicToolUseBlock, AnthropicUsage},
    },
    interfaces::ProviderStreamProjector,
};

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
                if !self.response_started
                    && let Some(event) = parse_message_start(value)
                {
                    self.response_started = true;
                    events.push(CanonicalStreamEvent::ResponseStarted {
                        model: event.message.model,
                        response_id: event.message.id,
                    });
                }
            }
            Some("content_block_start") => {
                if let Some(event) = parse_content_block_start(value) {
                    match json_str(&event.content_block, "type") {
                        Some("text") => {
                            let Some(_block) =
                                deserialize_wire::<AnthropicTextBlock>(&event.content_block)
                            else {
                                return Ok(events);
                            };
                            self.blocks
                                .insert(event.index, AnthropicBlockState::Text { item_id: None });
                            events.push(CanonicalStreamEvent::OutputItemStarted {
                                output_index: event.index,
                                item: StreamOutputItemStart::Message {
                                    item_id: None,
                                    role: MessageRole::Assistant,
                                },
                            });
                        }
                        Some("tool_use") => {
                            let Some(block) =
                                deserialize_wire::<AnthropicToolUseBlock>(&event.content_block)
                            else {
                                return Ok(events);
                            };
                            let name = block.name.unwrap_or_default();
                            let tool_call_id = block.id;
                            self.blocks.insert(
                                event.index,
                                AnthropicBlockState::ToolCall {
                                    item_id: None,
                                    tool_call_id: tool_call_id.clone(),
                                    name: name.clone(),
                                    arguments: String::new(),
                                },
                            );
                            events.push(CanonicalStreamEvent::OutputItemStarted {
                                output_index: event.index,
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
                if let Some(event) = parse_message_delta(value) {
                    if let Some(usage) = event.usage.as_ref().and_then(parse_anthropic_usage) {
                        events.push(CanonicalStreamEvent::UsageUpdated { usage });
                    }
                    self.stop_reason = parse_finish_reason(event.delta.stop_reason.as_deref());
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

fn json_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn json_u32(value: &Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(Value::as_u64)
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

fn parse_anthropic_usage(value: &Value) -> Option<Usage> {
    let usage = deserialize_wire::<AnthropicUsage>(value)?;
    Some(Usage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cached_input_tokens: Some(
            usage
                .cache_read_input_tokens
                .unwrap_or(0)
                .saturating_add(usage.cache_creation_input_tokens.unwrap_or(0)),
        ),
        total_tokens: None,
    })
}

fn deserialize_wire<T>(value: &Value) -> Option<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.clone()).ok()
}
