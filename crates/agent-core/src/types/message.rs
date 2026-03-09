use serde::{Deserialize, Serialize};

use super::tool::ContentPart;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentPart>,
}

impl Message {
    pub fn new(role: MessageRole, content: Vec<ContentPart>) -> Self {
        Self { role, content }
    }

    pub fn system_text(text: impl Into<String>) -> Self {
        Self::new(MessageRole::System, vec![ContentPart::text(text)])
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self::new(MessageRole::User, vec![ContentPart::text(text)])
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, vec![ContentPart::text(text)])
    }

    pub fn assistant_tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_json: serde_json::Value,
    ) -> Self {
        Self::new(
            MessageRole::Assistant,
            vec![ContentPart::tool_call(id, name, arguments_json)],
        )
    }

    pub fn tool_result_json(tool_call_id: impl Into<String>, value: serde_json::Value) -> Self {
        Self::new(
            MessageRole::Tool,
            vec![ContentPart::tool_result_json(tool_call_id, value)],
        )
    }

    pub fn tool_result_text(tool_call_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(
            MessageRole::Tool,
            vec![ContentPart::tool_result_text(tool_call_id, text)],
        )
    }

    pub fn tool_result_json_with_raw(
        tool_call_id: impl Into<String>,
        value: serde_json::Value,
        raw_provider_content: serde_json::Value,
    ) -> Self {
        Self::new(
            MessageRole::Tool,
            vec![ContentPart::tool_result_json_with_raw(
                tool_call_id,
                value,
                raw_provider_content,
            )],
        )
    }

    pub fn tool_result_text_with_raw(
        tool_call_id: impl Into<String>,
        text: impl Into<String>,
        raw_provider_content: serde_json::Value,
    ) -> Self {
        Self::new(
            MessageRole::Tool,
            vec![ContentPart::tool_result_text_with_raw(
                tool_call_id,
                text,
                raw_provider_content,
            )],
        )
    }
}
