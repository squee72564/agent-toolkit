use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::message::Message;
use super::tool::{ToolChoice, ToolDefinition};

/// Requested output format for a model response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Free-form text output.
    #[default]
    Text,
    /// A single JSON object emitted as text and later decoded by adapters or runtime helpers.
    JsonObject,
    /// A named JSON schema that providers should use for structured output when supported.
    JsonSchema {
        /// Stable schema name sent to providers that require one.
        name: String,
        /// JSON Schema object forwarded to providers.
        schema: serde_json::Value,
    },
}

/// Provider-agnostic request payload produced by the runtime and consumed by adapters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Provider model identifier to target.
    pub model_id: String,
    /// Whether the caller expects a streaming response.
    #[serde(default)]
    pub stream: bool,
    /// Ordered input messages.
    pub messages: Vec<Message>,
    /// Tool definitions exposed to the model for this request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    /// Tool invocation policy for the request.
    #[serde(default)]
    pub tool_choice: ToolChoice,
    /// Requested response encoding or structure.
    #[serde(default)]
    pub response_format: ResponseFormat,
    /// Sampling temperature when supported by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling configuration when supported by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Upper bound for generated output tokens when supported by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Stop sequences to forward to providers that support them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// Request-scoped metadata consumed by adapters and transports.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}
