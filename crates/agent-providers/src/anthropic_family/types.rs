//! Shared Anthropic-family wire types reused across decode and streaming paths.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Shared Anthropic decoded message envelope.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicMessageBody {
    /// Provider-generated message identifier.
    #[serde(default)]
    pub id: Option<String>,
    /// Top-level discriminator serialized from `type`.
    #[serde(default, rename = "type")]
    pub message_type: Option<String>,
    /// Role associated with the returned message.
    #[serde(default)]
    pub role: Option<String>,
    /// Model identifier returned by Anthropic.
    #[serde(default)]
    pub model: Option<String>,
    /// Raw content blocks returned in the message body.
    #[serde(default)]
    pub content: Vec<Value>,
    /// Anthropic stop reason for the completed message.
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// Optional provider stop-sequence payload.
    #[serde(default)]
    pub stop_sequence: Option<Value>,
    /// Raw usage payload returned alongside the message.
    #[serde(default)]
    pub usage: Option<Value>,
}

/// Shared Anthropic usage payload for token accounting.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AnthropicUsage {
    /// Count of prompt tokens reported by Anthropic.
    #[serde(default)]
    pub input_tokens: Option<u64>,
    /// Count of newly created cache tokens, when reported.
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    /// Count of cache-read prompt tokens, when reported.
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
    /// Count of completion tokens reported by Anthropic.
    #[serde(default)]
    pub output_tokens: Option<u64>,
}

/// Shared Anthropic top-level error envelope.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicErrorBody {
    /// Top-level discriminator serialized from `type`.
    #[serde(default, rename = "type")]
    pub body_type: Option<String>,
    /// Nested provider error payload.
    #[serde(default)]
    pub error: Option<AnthropicErrorPayload>,
    /// Optional provider request identifier.
    #[serde(default)]
    pub request_id: Option<Value>,
}

/// Shared Anthropic nested error payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicErrorPayload {
    /// Provider-reported error message, which may be non-string on the wire.
    #[serde(default)]
    pub message: Option<Value>,
    /// Provider-reported error type serialized from `type`.
    #[serde(default, rename = "type")]
    pub error_type: Option<Value>,
}

/// Known Anthropic text content block.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicTextBlock {
    /// Block discriminator serialized from `type`.
    #[serde(rename = "type")]
    pub block_type: String,
    /// Text payload emitted by the block.
    #[serde(default)]
    pub text: Option<String>,
}

/// Known Anthropic tool-use content block.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicToolUseBlock {
    /// Block discriminator serialized from `type`.
    #[serde(rename = "type")]
    pub block_type: String,
    /// Provider-generated tool call identifier.
    #[serde(default)]
    pub id: Option<String>,
    /// Tool name selected by the model.
    #[serde(default)]
    pub name: Option<String>,
    /// Tool input payload emitted by the model.
    #[serde(default)]
    pub input: Option<Value>,
}

/// Known Anthropic thinking content block.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicThinkingBlock {
    /// Block discriminator serialized from `type`.
    #[serde(rename = "type")]
    pub block_type: String,
    /// Thinking text when Anthropic uses an explicit `thinking` field.
    #[serde(default)]
    pub thinking: Option<String>,
    /// Alternate text field seen on some thinking-like payloads.
    #[serde(default)]
    pub text: Option<String>,
}

/// Known Anthropic redacted-thinking content block.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AnthropicRedactedThinkingBlock {
    /// Block discriminator serialized from `type`.
    #[serde(rename = "type")]
    pub block_type: String,
}
