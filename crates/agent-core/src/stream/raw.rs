use serde::{Deserialize, Serialize};

use crate::types::ProviderKind;

/// Transport envelope for a raw streaming event before it is normalized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawStreamTransport {
    /// A Server-Sent Events frame.
    Sse {
        /// Optional SSE event name.
        #[serde(skip_serializing_if = "Option::is_none")]
        event: Option<String>,
        /// Optional SSE event id.
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Optional SSE retry value in milliseconds.
        #[serde(skip_serializing_if = "Option::is_none")]
        retry: Option<u64>,
    },
}

/// Classified payload body of a raw streaming frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawStreamPayload {
    /// JSON payload parsed from the raw frame body.
    Json { value: serde_json::Value },
    /// Non-JSON text payload.
    Text { text: String },
    /// Stream terminator such as the SSE `[DONE]` sentinel.
    Done,
    /// Empty payload body.
    Empty,
}

/// Provider-specific stream frame plus minimal transport metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRawStreamEvent {
    /// Provider that emitted the frame.
    pub provider: ProviderKind,
    /// Monotonic sequence number assigned by the transport.
    pub sequence: u64,
    /// Transport metadata for the frame.
    pub transport: RawStreamTransport,
    /// Classified payload body.
    pub payload: RawStreamPayload,
}

impl ProviderRawStreamEvent {
    /// Creates a raw stream event from a single SSE frame and classifies its `data` payload.
    pub fn from_sse(
        provider: ProviderKind,
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

    /// Returns the parsed JSON payload when the frame body was valid JSON.
    pub fn json(&self) -> Option<&serde_json::Value> {
        match &self.payload {
            RawStreamPayload::Json { value } => Some(value),
            _ => None,
        }
    }

    /// Returns the SSE event name for SSE-backed transports.
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
