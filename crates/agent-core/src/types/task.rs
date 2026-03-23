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

/// Semantic request content independent of route selection and execution mode.
///
/// This type is semantic-only and intentionally limited to request intent that
/// should survive provider selection unchanged. Keep tuning, token budgets,
/// stop controls, metadata, and provider execution controls out of
/// [`TaskRequest`].
///
/// Use [`crate::NativeOptions`] to express request controls:
///
/// - [`crate::FamilyOptions`] for controls shared by one provider family
/// - [`crate::ProviderOptions`] for provider-native or router-native controls
///
/// Direct-provider runtime helpers accept these typed native options alongside
/// semantic input, for example `openai().create_with_openai_options(...)`,
/// `anthropic().create_with_anthropic_options(...)`, and
/// `openrouter().create_with_openrouter_options(...)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRequest {
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
}
