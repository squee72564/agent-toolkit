use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{FinishReason, MessageRole, ProviderId, Usage};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawStreamTransport {
    Sse {
        #[serde(skip_serializing_if = "Option::is_none")]
        event: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        retry: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawStreamPayload {
    Json { value: serde_json::Value },
    Text { text: String },
    Done,
    Comment { text: String },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRawStreamEvent {
    pub provider: ProviderId,
    pub sequence: u64,
    pub transport: RawStreamTransport,
    pub payload: RawStreamPayload,
}

impl ProviderRawStreamEvent {
    pub fn from_sse(
        provider: ProviderId,
        sequence: u64,
        event: Option<String>,
        id: Option<String>,
        retry: Option<u64>,
        data: impl Into<String>,
    ) -> Self {
        let data = data.into();
        let payload = Self::classify_sse_payload(&data);

        Self {
            provider,
            sequence,
            transport: RawStreamTransport::Sse { event, id, retry },
            payload,
        }
    }

    pub fn from_sse_comment(provider: ProviderId, sequence: u64, text: impl Into<String>) -> Self {
        Self {
            provider,
            sequence,
            transport: RawStreamTransport::Sse {
                event: None,
                id: None,
                retry: None,
            },
            payload: RawStreamPayload::Comment { text: text.into() },
        }
    }

    pub fn json(&self) -> Option<&serde_json::Value> {
        match &self.payload {
            RawStreamPayload::Json { value } => Some(value),
            _ => None,
        }
    }

    pub fn sse_event_name(&self) -> Option<&str> {
        match &self.transport {
            RawStreamTransport::Sse { event, .. } => event.as_deref(),
        }
    }

    fn classify_sse_payload(data: &str) -> RawStreamPayload {
        if data == "[DONE]" {
            RawStreamPayload::Done
        } else if data.is_empty() {
            RawStreamPayload::Empty
        } else if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
            RawStreamPayload::Json { value }
        } else {
            RawStreamPayload::Text {
                text: data.to_string(),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalStreamEnvelope {
    pub raw: ProviderRawStreamEvent,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canonical: Vec<CanonicalStreamEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanonicalStreamEvent {
    ResponseStarted {
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
    },
    OutputItemStarted {
        output_index: u32,
        item: StreamOutputItemStart,
    },
    TextDelta {
        output_index: u32,
        content_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        delta: String,
    },
    ToolCallArgumentsDelta {
        output_index: u32,
        tool_call_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        delta: String,
    },
    OutputItemCompleted {
        output_index: u32,
        item: StreamOutputItemEnd,
    },
    UsageUpdated {
        usage: Usage,
    },
    Completed {
        finish_reason: FinishReason,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamOutputItemStart {
    Message {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        role: MessageRole,
    },
    ToolCall {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamOutputItemEnd {
    Message {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
    },
    ToolCall {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        name: String,
        arguments_json_text: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct CanonicalStreamProjector {
    response_started: bool,
    completed: bool,
    anthropic_stop_reason: Option<FinishReason>,
    anthropic_blocks: BTreeMap<u32, AnthropicBlockState>,
    openrouter_message_items: BTreeMap<u32, OpenRouterMessageState>,
    openrouter_tool_calls: BTreeMap<(u32, u32), OpenRouterToolCallState>,
}

impl CanonicalStreamProjector {
    pub fn project(&mut self, raw: ProviderRawStreamEvent) -> CanonicalStreamEnvelope {
        let canonical = match raw.provider {
            ProviderId::OpenAi => self.project_openai(&raw),
            ProviderId::Anthropic => self.project_anthropic(&raw),
            ProviderId::OpenRouter => self.project_openrouter(&raw),
        };

        CanonicalStreamEnvelope { raw, canonical }
    }

    fn project_openai(&mut self, raw: &ProviderRawStreamEvent) -> Vec<CanonicalStreamEvent> {
        let Some(value) = raw.json() else {
            return Vec::new();
        };

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

                    if let Some(usage) = response.get("usage").and_then(parse_openai_usage) {
                        events.push(CanonicalStreamEvent::UsageUpdated { usage });
                    }

                    if !self.completed {
                        self.completed = true;
                        events.push(CanonicalStreamEvent::Completed {
                            finish_reason: infer_openai_finish_reason(response),
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

    fn project_anthropic(&mut self, raw: &ProviderRawStreamEvent) -> Vec<CanonicalStreamEvent> {
        let Some(value) = raw.json() else {
            return Vec::new();
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
                            self.anthropic_blocks
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
                            self.anthropic_blocks.insert(
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
                                        .anthropic_blocks
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
                                }) = self.anthropic_blocks.get_mut(&index)
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
                    && let Some(block) = self.anthropic_blocks.remove(&index)
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
                    self.anthropic_stop_reason =
                        parse_finish_reason(json_str(delta, "stop_reason"));
                }
            }
            Some("message_stop") => {
                if !self.completed {
                    self.completed = true;
                    events.push(CanonicalStreamEvent::Completed {
                        finish_reason: self
                            .anthropic_stop_reason
                            .clone()
                            .unwrap_or(FinishReason::Other),
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

        events
    }

    fn project_openrouter(&mut self, raw: &ProviderRawStreamEvent) -> Vec<CanonicalStreamEvent> {
        match &raw.payload {
            RawStreamPayload::Comment { .. }
            | RawStreamPayload::Empty
            | RawStreamPayload::Text { .. } => Vec::new(),
            RawStreamPayload::Done => {
                if self.completed {
                    return Vec::new();
                }
                self.completed = true;
                vec![CanonicalStreamEvent::Completed {
                    finish_reason: FinishReason::Other,
                }]
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
                                self.openrouter_message_items.entry(output_index)
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
                                let state = self.openrouter_tool_calls.entry(key).or_default();

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
                        if let Some(message) = self.openrouter_message_items.get_mut(&output_index)
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
                            .openrouter_tool_calls
                            .keys()
                            .copied()
                            .filter(|(choice_index, _)| *choice_index == output_index)
                            .collect();
                        for key in tool_keys {
                            if let Some(state) = self.openrouter_tool_calls.remove(&key)
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

                events
            }
        }
    }
}

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
