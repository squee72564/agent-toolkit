use serde::{Deserialize, Serialize};

use super::tool::ContentPart;

/// Logical role assigned to a message in a conversational request or transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageRole {
    /// Provider instructions that shape subsequent generation.
    System,
    /// End-user input.
    User,
    /// Model-authored content, including text and tool calls.
    Assistant,
    /// Tool output fed back into the conversation.
    Tool,
}

/// A provider-agnostic message composed of ordered content parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    /// The role that owns the message.
    pub role: MessageRole,
    /// The content parts in provider-independent form.
    pub content: Vec<ContentPart>,
}

impl Message {
    /// Creates a message with the provided role and content parts.
    pub fn new(role: MessageRole, content: Vec<ContentPart>) -> Self {
        Self { role, content }
    }

    /// Creates a system message containing a single text part.
    pub fn system_text(text: impl Into<String>) -> Self {
        Self::new(MessageRole::System, vec![ContentPart::text(text)])
    }

    /// Creates a user message containing a single text part.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::new(MessageRole::User, vec![ContentPart::text(text)])
    }

    /// Creates an assistant message containing a single text part.
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, vec![ContentPart::text(text)])
    }

    /// Creates an assistant message containing a single tool call part.
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

    /// Creates a tool message with a single JSON result part.
    pub fn tool_result_json(tool_call_id: impl Into<String>, value: serde_json::Value) -> Self {
        Self::new(
            MessageRole::Tool,
            vec![ContentPart::tool_result_json(tool_call_id, value)],
        )
    }

    /// Creates a tool message with a single text result part.
    pub fn tool_result_text(tool_call_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(
            MessageRole::Tool,
            vec![ContentPart::tool_result_text(tool_call_id, text)],
        )
    }

    /// Creates a tool message with a JSON result and provider-native payload attached.
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

    /// Creates a tool message with a text result and provider-native payload attached.
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
