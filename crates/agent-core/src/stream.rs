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
