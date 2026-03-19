//! Shared OpenAI-family wire types reused across provider adapters.
//!
//! These are protocol-level payload fragments used by the OpenAI-compatible
//! family encoder and decoder. They intentionally model the wire contract, not
//! the higher-level canonical request/response types exposed by `agent-core`.

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
    /// Normalizes schema defaults used across OpenAI-family request encoders.
    ///
    /// When the schema omits `additionalProperties`, this helper inserts
    /// `false` so structured outputs default to closed object shapes.
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
    /// Plain-text output.
    #[serde(rename = "text")]
    Text,
    /// JSON schema constrained output.
    #[serde(rename = "json_schema")]
    JsonSchema,
    /// Arbitrary JSON object output.
    #[serde(rename = "json_object")]
    JsonObject,
}

/// Responses API `text.format` payload.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OpenAiTextFormat {
    /// Output format discriminator serialized as `text.format.type`.
    #[serde(rename = "type")]
    pub format_type: OpenAiTextFormatType,
    /// Optional schema name used for `json_schema` outputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional JSON schema used for `json_schema` outputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    /// Optional strictness flag used for `json_schema` outputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for OpenAiTextFormat {
    fn default() -> Self {
        Self::text()
    }
}

impl OpenAiTextFormat {
    /// Builds the plain-text `text.format` payload.
    #[must_use]
    pub fn text() -> Self {
        Self {
            format_type: OpenAiTextFormatType::Text,
            name: None,
            schema: None,
            strict: None,
        }
    }

    /// Builds the JSON-object `text.format` payload.
    #[must_use]
    pub fn json_object() -> Self {
        Self {
            format_type: OpenAiTextFormatType::JsonObject,
            name: None,
            schema: None,
            strict: None,
        }
    }

    /// Builds the JSON-schema `text.format` payload.
    ///
    /// This preserves the schema name and strictness settings, and normalizes
    /// the schema with [`StructuredOutputFormat::with_default_additional_properties_false`].
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
    /// Function tool definition.
    #[serde(rename = "function")]
    Function,
}

/// OpenAI-family function tool payload.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OpenAiFunctionToolDefinition {
    /// Tool discriminator serialized as `function`.
    #[serde(rename = "type")]
    pub tool_type: OpenAiToolType,
    /// Tool name exposed to the model.
    pub name: String,
    /// Optional tool description exposed to the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON schema for the tool parameters.
    pub parameters: Value,
    /// Optional strict schema flag for parameter validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Shared OpenAI-family error payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OpenAiErrorEnvelope {
    /// Provider-reported error message, which may be non-string on the wire.
    #[serde(default)]
    pub message: Option<Value>,
    /// Provider-reported error code, if present.
    #[serde(default)]
    pub code: Option<Value>,
    /// Provider-reported error type, serialized from the `type` field.
    #[serde(default, rename = "type")]
    pub error_type: Option<Value>,
    /// Provider-reported parameter name associated with the error.
    #[serde(default)]
    pub param: Option<Value>,
}

/// Shared OpenAI-family incomplete-details payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiIncompleteDetails {
    /// Provider-specific reason explaining why generation was incomplete.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Shared OpenAI-family usage detail payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiUsageTokenDetails {
    /// Count of prompt tokens served from cache, when reported.
    #[serde(default)]
    pub cached_tokens: Option<u64>,
}

/// Shared OpenAI-family usage payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiResponseUsage {
    /// Total prompt/input tokens consumed.
    #[serde(default)]
    pub input_tokens: Option<u64>,
    /// Total completion/output tokens produced.
    #[serde(default)]
    pub output_tokens: Option<u64>,
    /// Total combined tokens reported by the provider.
    #[serde(default)]
    pub total_tokens: Option<u64>,
    /// Nested prompt-token details such as cached token counts.
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
    /// High-level response status such as `completed` or `incomplete`.
    #[serde(default)]
    pub status: Option<String>,
    /// Model identifier returned by the provider.
    #[serde(default)]
    pub model: Option<String>,
    /// Raw output items array returned by the Responses API.
    #[serde(default)]
    pub output: Option<Value>,
    /// Token-usage block, when present.
    #[serde(default)]
    pub usage: Option<OpenAiResponseUsage>,
    /// Additional details for incomplete responses.
    #[serde(default)]
    pub incomplete_details: Option<OpenAiIncompleteDetails>,
    /// Embedded provider error payload, when the response reports one.
    #[serde(default)]
    pub error: Option<OpenAiErrorEnvelope>,
}

/// OpenAI-family `output` item representing an assistant message.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OpenAiMessageOutputItem {
    /// Provider-generated item identifier, when present.
    #[serde(default)]
    pub id: Option<String>,
    /// Item type discriminator, expected to be `message`.
    #[serde(rename = "type")]
    pub item_type: String,
    /// Provider-reported item status such as `completed`.
    #[serde(default)]
    pub status: Option<String>,
    /// Role associated with the message item.
    #[serde(default)]
    pub role: Option<String>,
    /// Nested message content parts.
    #[serde(default)]
    pub content: Vec<Value>,
}

/// OpenAI-family `output` item representing a function call.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OpenAiFunctionCallOutputItem {
    /// Provider-generated item identifier, when present.
    #[serde(default)]
    pub id: Option<String>,
    /// Item type discriminator, expected to be `function_call`.
    #[serde(rename = "type")]
    pub item_type: String,
    /// Provider-reported item status such as `completed`.
    #[serde(default)]
    pub status: Option<String>,
    /// JSON-encoded function arguments text.
    #[serde(default)]
    pub arguments: Option<String>,
    /// Provider-generated tool call identifier.
    #[serde(default)]
    pub call_id: Option<String>,
    /// Tool name selected by the model.
    #[serde(default)]
    pub name: Option<String>,
}

/// OpenAI-family `output` item representing reasoning metadata.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OpenAiReasoningOutputItem {
    /// Provider-generated item identifier, when present.
    #[serde(default)]
    pub id: Option<String>,
    /// Item type discriminator, expected to be `reasoning`.
    #[serde(rename = "type")]
    pub item_type: String,
    /// Provider-specific reasoning summary payload.
    #[serde(default)]
    pub summary: Option<Value>,
}

/// OpenAI-family `output` item representing a refusal payload.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OpenAiRefusalOutputItem {
    /// Provider-generated item identifier, when present.
    #[serde(default)]
    pub id: Option<String>,
    /// Item type discriminator, expected to be `refusal`.
    #[serde(rename = "type")]
    pub item_type: String,
    /// Refusal text when surfaced in a `text` field.
    #[serde(default)]
    pub text: Option<String>,
    /// Refusal text when surfaced in a `refusal` field.
    #[serde(default)]
    pub refusal: Option<String>,
}

/// OpenAI-family assistant message content part.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum OpenAiMessageContentPart {
    /// Plain text output content.
    #[serde(rename = "output_text")]
    OutputText {
        /// Text emitted for the content part.
        #[serde(default)]
        text: Option<String>,
    },
    /// Refusal content part.
    #[serde(rename = "refusal")]
    Refusal {
        /// Refusal text when surfaced in a `text` field.
        #[serde(default)]
        text: Option<String>,
        /// Refusal text when surfaced in a `refusal` field.
        #[serde(default)]
        refusal: Option<String>,
    },
    /// Reasoning content part.
    #[serde(rename = "reasoning")]
    Reasoning,
}

impl OpenAiMessageContentPart {
    /// Returns the text for `output_text` content parts.
    #[must_use]
    pub fn output_text(&self) -> Option<&str> {
        match self {
            Self::OutputText { text } => text.as_deref(),
            _ => None,
        }
    }

    /// Returns refusal text for refusal content parts.
    #[must_use]
    pub fn refusal_text(&self) -> Option<&str> {
        match self {
            Self::Refusal { text, refusal } => text
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .or_else(|| refusal.as_deref().filter(|text| !text.trim().is_empty())),
            _ => None,
        }
    }
}
