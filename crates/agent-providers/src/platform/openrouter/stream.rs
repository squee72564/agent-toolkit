use std::collections::BTreeMap;

use agent_core::{
    CanonicalStreamEvent, FinishReason, MessageRole, ProviderRawStreamEvent, RawStreamPayload,
    StreamOutputItemEnd, StreamOutputItemStart, Usage,
};

use crate::error::AdapterError;
use crate::streaming::ProviderStreamProjector;

#[derive(Debug, Clone)]
struct OpenRouterMessageState {
    item_id: Option<String>,
    completed: bool,
}

#[derive(Debug, Clone, Default)]
struct OpenRouterToolCallState {
    item_id: Option<String>,
    tool_call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Default)]
pub(crate) struct OpenRouterStreamProjector {
    response_started: bool,
    completed: bool,
    message_items: BTreeMap<u32, OpenRouterMessageState>,
    tool_calls: BTreeMap<(u32, u32), OpenRouterToolCallState>,
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
                let mut events = Vec::new();

                if !self.response_started {
                    self.response_started = true;
                    events.push(CanonicalStreamEvent::ResponseStarted {
                        model: json_str(value, "model").map(ToOwned::to_owned),
                        response_id: json_str(value, "id").map(ToOwned::to_owned),
                    });
                }

                if let Some(usage) = value.get("usage").and_then(parse_openrouter_usage) {
                    events.push(CanonicalStreamEvent::UsageUpdated { usage });
                }

                let choices = value
                    .get("choices")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();

                for choice in choices {
                    let output_index = choice
                        .get("index")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or(0);

                    if let Some(delta) = choice.get("delta") {
                        if let Some(content) = json_str(delta, "content")
                            && !content.is_empty()
                        {
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                self.message_items.entry(output_index)
                            {
                                entry.insert(OpenRouterMessageState {
                                    item_id: None,
                                    completed: false,
                                });
                                events.push(CanonicalStreamEvent::OutputItemStarted {
                                    output_index,
                                    item: StreamOutputItemStart::Message {
                                        item_id: None,
                                        role: MessageRole::Assistant,
                                    },
                                });
                            }
                            events.push(CanonicalStreamEvent::TextDelta {
                                output_index,
                                content_index: 0,
                                item_id: None,
                                delta: content.to_string(),
                            });
                        }

                        if let Some(tool_calls) = delta
                            .get("tool_calls")
                            .and_then(serde_json::Value::as_array)
                        {
                            for (fallback_index, tool_call) in tool_calls.iter().enumerate() {
                                let tool_call_index = tool_call
                                    .get("index")
                                    .and_then(serde_json::Value::as_u64)
                                    .and_then(|value| u32::try_from(value).ok())
                                    .unwrap_or_else(|| u32::try_from(fallback_index).unwrap_or(0));
                                let key = (output_index, tool_call_index);
                                let state = self.tool_calls.entry(key).or_default();

                                let mut should_start = false;
                                if state.name.is_none() {
                                    state.name = tool_call
                                        .get("function")
                                        .and_then(|function| json_str(function, "name"))
                                        .map(ToOwned::to_owned);
                                    should_start = state.name.is_some();
                                }
                                if state.tool_call_id.is_none() {
                                    state.tool_call_id =
                                        json_str(tool_call, "id").map(ToOwned::to_owned);
                                }
                                if state.item_id.is_none() {
                                    state.item_id =
                                        json_str(tool_call, "id").map(ToOwned::to_owned);
                                }
                                if should_start {
                                    events.push(CanonicalStreamEvent::OutputItemStarted {
                                        output_index,
                                        item: StreamOutputItemStart::ToolCall {
                                            item_id: state.item_id.clone(),
                                            tool_call_id: state.tool_call_id.clone(),
                                            name: state.name.clone().unwrap_or_default(),
                                        },
                                    });
                                }
                                if let Some(arguments) = tool_call
                                    .get("function")
                                    .and_then(|function| json_str(function, "arguments"))
                                {
                                    state.arguments.push_str(arguments);
                                    events.push(CanonicalStreamEvent::ToolCallArgumentsDelta {
                                        output_index,
                                        tool_call_index,
                                        item_id: state.item_id.clone(),
                                        tool_call_id: state.tool_call_id.clone(),
                                        tool_name: state.name.clone(),
                                        delta: arguments.to_string(),
                                    });
                                }
                            }
                        }
                    }

                    if let Some(finish_reason) = parse_finish_reason(
                        choice
                            .get("finish_reason")
                            .and_then(serde_json::Value::as_str),
                    ) {
                        if let Some(message) = self.message_items.get_mut(&output_index)
                            && !message.completed
                        {
                            message.completed = true;
                            events.push(CanonicalStreamEvent::OutputItemCompleted {
                                output_index,
                                item: StreamOutputItemEnd::Message {
                                    item_id: message.item_id.clone(),
                                },
                            });
                        }

                        let tool_keys: Vec<_> = self
                            .tool_calls
                            .keys()
                            .copied()
                            .filter(|(choice_index, _)| *choice_index == output_index)
                            .collect();
                        for key in tool_keys {
                            if let Some(state) = self.tool_calls.remove(&key)
                                && let Some(name) = state.name
                            {
                                events.push(CanonicalStreamEvent::OutputItemCompleted {
                                    output_index,
                                    item: StreamOutputItemEnd::ToolCall {
                                        item_id: state.item_id,
                                        tool_call_id: state.tool_call_id,
                                        name,
                                        arguments_json_text: state.arguments,
                                    },
                                });
                            }
                        }

                        if !self.completed {
                            self.completed = true;
                            events.push(CanonicalStreamEvent::Completed { finish_reason });
                        }
                    }
                }

                Ok(events)
            }
        }
    }
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
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

fn parse_openrouter_usage(value: &serde_json::Value) -> Option<Usage> {
    Some(Usage {
        input_tokens: value
            .get("prompt_tokens")
            .and_then(serde_json::Value::as_u64),
        output_tokens: value
            .get("completion_tokens")
            .and_then(serde_json::Value::as_u64),
        cached_input_tokens: value
            .get("prompt_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(serde_json::Value::as_u64),
        total_tokens: value
            .get("total_tokens")
            .and_then(serde_json::Value::as_u64),
    })
}
