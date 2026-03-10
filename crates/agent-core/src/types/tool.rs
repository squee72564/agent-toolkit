use serde::{Deserialize, Serialize};

/// Tool selection policy to send to a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    /// Disable tool use for the request.
    None,
    /// Let the model decide whether to call a tool.
    #[default]
    Auto,
    /// Require the model to call a tool before completing.
    Required,
    /// Force selection of a specific tool by name.
    Specific {
        /// Name of the required tool.
        name: String,
    },
}

/// Provider-agnostic tool definition exposed to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDefinition {
    /// Stable tool name referenced by tool choice and tool calls.
    pub name: String,
    /// Optional human-readable description shown to the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema object describing the tool input shape.
    pub parameters_schema: serde_json::Value,
}

/// Canonical representation of tool output content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultContent {
    /// Plain text tool output.
    Text { text: String },
    /// Structured JSON tool output.
    Json { value: serde_json::Value },
    /// Provider-neutral multipart output used by adapters that support richer tool results.
    Parts { parts: Vec<ContentPart> },
}

/// A model-emitted tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    /// Provider- or runtime-assigned identifier for matching results to the call.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Parsed JSON arguments supplied by the model.
    pub arguments_json: serde_json::Value,
}

/// Tool output returned to a model in a follow-up message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolResult {
    /// Identifier of the tool call this result satisfies.
    pub tool_call_id: String,
    /// Canonicalized result payload.
    pub content: ToolResultContent,
    /// Optional provider-native payload preserved for adapters that can forward it directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_provider_content: Option<serde_json::Value>,
}

/// A single piece of message content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Plain text.
    Text { text: String },
    /// A model-issued tool call.
    ToolCall { tool_call: ToolCall },
    /// A tool result included in a follow-up tool message.
    ToolResult { tool_result: ToolResult },
}

impl ContentPart {
    /// Creates a text content part.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Creates a tool call content part.
    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_json: serde_json::Value,
    ) -> Self {
        Self::ToolCall {
            tool_call: ToolCall {
                id: id.into(),
                name: name.into(),
                arguments_json,
            },
        }
    }

    /// Creates a JSON tool result content part.
    pub fn tool_result_json(tool_call_id: impl Into<String>, value: serde_json::Value) -> Self {
        Self::ToolResult {
            tool_result: ToolResult {
                tool_call_id: tool_call_id.into(),
                content: ToolResultContent::Json { value },
                raw_provider_content: None,
            },
        }
    }

    /// Creates a text tool result content part.
    pub fn tool_result_text(tool_call_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_result: ToolResult {
                tool_call_id: tool_call_id.into(),
                content: ToolResultContent::Text { text: text.into() },
                raw_provider_content: None,
            },
        }
    }

    /// Creates a JSON tool result content part with provider-native content attached.
    pub fn tool_result_json_with_raw(
        tool_call_id: impl Into<String>,
        value: serde_json::Value,
        raw_provider_content: serde_json::Value,
    ) -> Self {
        Self::ToolResult {
            tool_result: ToolResult {
                tool_call_id: tool_call_id.into(),
                content: ToolResultContent::Json { value },
                raw_provider_content: Some(raw_provider_content),
            },
        }
    }

    /// Creates a text tool result content part with provider-native content attached.
    pub fn tool_result_text_with_raw(
        tool_call_id: impl Into<String>,
        text: impl Into<String>,
        raw_provider_content: serde_json::Value,
    ) -> Self {
        Self::ToolResult {
            tool_result: ToolResult {
                tool_call_id: tool_call_id.into(),
                content: ToolResultContent::Text { text: text.into() },
                raw_provider_content: Some(raw_provider_content),
            },
        }
    }
}
