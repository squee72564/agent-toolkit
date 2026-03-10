use serde::{Deserialize, Serialize};

use super::tool::ContentPart;

/// Final assistant output returned by a provider adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantOutput {
    /// Ordered assistant content parts such as text and tool calls.
    pub content: Vec<ContentPart>,
    /// Parsed structured output when a JSON response format was requested and decoding succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Value>,
}

/// Token accounting reported by providers or derived by the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    /// Input prompt tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Generated output tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Prompt tokens served from cache when a provider exposes that detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    /// Provider-reported total token count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

impl Usage {
    /// Returns `total_tokens` when present, otherwise derives it as `input_tokens + output_tokens`.
    pub fn derived_total_tokens(&self) -> u64 {
        self.total_tokens.unwrap_or_else(|| {
            self.input_tokens
                .unwrap_or(0)
                .saturating_add(self.output_tokens.unwrap_or(0))
        })
    }
}

/// Provider-independent reason a response finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FinishReason {
    /// The model finished normally.
    Stop,
    /// Generation stopped because a token limit was reached.
    Length,
    /// The model stopped to return tool calls.
    ToolCalls,
    /// The provider filtered the response content.
    ContentFilter,
    /// The response terminated because of an error.
    Error,
    /// The provider exposed a finish state that does not map cleanly to the canonical enum.
    Other,
}

/// Non-fatal issue produced while encoding, decoding, or normalizing a request or response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeWarning {
    /// Stable warning code for programmatic handling.
    pub code: String,
    /// Human-readable warning detail.
    pub message: String,
}

/// Normalized provider response returned to the runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    /// Assistant output content and any decoded structured payload.
    pub output: AssistantOutput,
    /// Usage totals reported by the provider or runtime.
    pub usage: Usage,
    /// Model identifier returned by the provider, which may differ from the requested model.
    pub model: String,
    /// Provider-native response payload retained for debugging or downstream inspection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_provider_response: Option<serde_json::Value>,
    /// Canonical completion reason.
    pub finish_reason: FinishReason,
    /// Non-fatal warnings accumulated during normalization.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<RuntimeWarning>,
}
