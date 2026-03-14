//! Shared OpenAI-family wire types reused across provider adapters.

use agent_core::Usage;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Structured output definition compatible with OpenAI-style JSON schema outputs.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StructuredOutputFormat {
    /// Name of the schema.
    pub name: String,
    /// Optional human-readable schema description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The JSON schema for the structured output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    /// Whether to enable strict schema adherence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl StructuredOutputFormat {
    /// Normalize schema defaults used across OpenAI-family request encoders.
    #[must_use]
    pub fn with_default_additional_properties_false(mut self) -> Self {
        if let Some(schema) = self.schema.as_mut()
            && schema.get("additionalProperties").is_none()
        {
            schema["additionalProperties"] = serde_json::json!(false);
        }
        self
    }
}

/// Responses API text-format discriminator.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum OpenAiTextFormatType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "json_schema")]
    JsonSchema,
    #[serde(rename = "json_object")]
    JsonObject,
}

/// Responses API `text.format` payload.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OpenAiTextFormat {
    #[serde(rename = "type")]
    pub format_type: OpenAiTextFormatType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for OpenAiTextFormat {
    fn default() -> Self {
        Self::text()
    }
}

impl OpenAiTextFormat {
    #[must_use]
    pub fn text() -> Self {
        Self {
            format_type: OpenAiTextFormatType::Text,
            name: None,
            schema: None,
            strict: None,
        }
    }

    #[must_use]
    pub fn json_object() -> Self {
        Self {
            format_type: OpenAiTextFormatType::JsonObject,
            name: None,
            schema: None,
            strict: None,
        }
    }

    #[must_use]
    pub fn json_schema(schema: StructuredOutputFormat) -> Self {
        let schema = schema.with_default_additional_properties_false();
        Self {
            format_type: OpenAiTextFormatType::JsonSchema,
            name: Some(schema.name),
            schema: schema.schema,
            strict: schema.strict,
        }
    }
}

/// OpenAI-family tool type.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum OpenAiToolType {
    #[serde(rename = "function")]
    Function,
}

/// OpenAI-family function tool payload.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OpenAiFunctionToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: OpenAiToolType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Shared OpenAI-family error payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OpenAiErrorEnvelope {
    #[serde(default)]
    pub message: Option<Value>,
    #[serde(default)]
    pub code: Option<Value>,
    #[serde(default, rename = "type")]
    pub error_type: Option<Value>,
    #[serde(default)]
    pub param: Option<Value>,
}

/// Shared OpenAI-family incomplete-details payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiIncompleteDetails {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Shared OpenAI-family usage detail payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiUsageTokenDetails {
    #[serde(default)]
    pub cached_tokens: Option<u64>,
}

/// Shared OpenAI-family usage payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiResponseUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub input_tokens_details: Option<OpenAiUsageTokenDetails>,
}

impl From<OpenAiResponseUsage> for Usage {
    fn from(value: OpenAiResponseUsage) -> Self {
        Self {
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            cached_input_tokens: value
                .input_tokens_details
                .and_then(|details| details.cached_tokens),
            total_tokens: value.total_tokens,
        }
    }
}

/// Shared OpenAI-family decoded Responses API envelope.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OpenAiResponsesBody {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub usage: Option<OpenAiResponseUsage>,
    #[serde(default)]
    pub incomplete_details: Option<OpenAiIncompleteDetails>,
    #[serde(default)]
    pub error: Option<OpenAiErrorEnvelope>,
}
