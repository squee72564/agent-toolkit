use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::AnthropicMessageBody;

/// Typed Anthropic `message_start` streaming event.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicMessageStartEvent {
    /// Event discriminator serialized from `type`.
    #[serde(default, rename = "type")]
    pub event_type: Option<String>,
    /// Nested message payload included with the start event.
    pub message: AnthropicMessageBody,
}

/// Typed Anthropic `content_block_start` streaming event.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicContentBlockStartEvent {
    /// Event discriminator serialized from `type`.
    #[serde(default, rename = "type")]
    pub event_type: Option<String>,
    /// Output index for the content block.
    pub index: u32,
    /// Raw content block payload emitted by Anthropic.
    pub content_block: Value,
}

/// Typed Anthropic `message_delta` streaming event.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicMessageDeltaEvent {
    /// Event discriminator serialized from `type`.
    #[serde(default, rename = "type")]
    pub event_type: Option<String>,
    /// Nested delta payload for the message.
    pub delta: AnthropicMessageDeltaPayload,
    /// Raw usage payload reported with the delta.
    #[serde(default)]
    pub usage: Option<Value>,
}

/// Typed payload for Anthropic `message_delta.delta`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicMessageDeltaPayload {
    /// Updated stop reason, when Anthropic reports one.
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// Optional provider stop-sequence payload.
    #[serde(default)]
    pub stop_sequence: Option<Value>,
}

pub(crate) fn parse_message_start(value: &Value) -> Option<AnthropicMessageStartEvent> {
    deserialize_wire(value)
}

pub(crate) fn parse_content_block_start(value: &Value) -> Option<AnthropicContentBlockStartEvent> {
    deserialize_wire(value)
}

pub(crate) fn parse_message_delta(value: &Value) -> Option<AnthropicMessageDeltaEvent> {
    deserialize_wire(value)
}

fn deserialize_wire<T>(value: &Value) -> Option<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.clone()).ok()
}
