use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    None,
    #[default]
    Auto,
    Required,
    Specific {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters_schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultContent {
    Text { text: String },
    Json { value: serde_json::Value },
    Parts { parts: Vec<ContentPart> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: ToolResultContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_provider_content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ToolCall { tool_call: ToolCall },
    ToolResult { tool_result: ToolResult },
}

impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

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

    pub fn tool_result_json(tool_call_id: impl Into<String>, value: serde_json::Value) -> Self {
        Self::ToolResult {
            tool_result: ToolResult {
                tool_call_id: tool_call_id.into(),
                content: ToolResultContent::Json { value },
                raw_provider_content: None,
            },
        }
    }

    pub fn tool_result_text(tool_call_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_result: ToolResult {
                tool_call_id: tool_call_id.into(),
                content: ToolResultContent::Text { text: text.into() },
                raw_provider_content: None,
            },
        }
    }

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
