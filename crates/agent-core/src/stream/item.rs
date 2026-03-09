use serde::{Deserialize, Serialize};

use crate::types::MessageRole;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamOutputItemStart {
    Message {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        role: MessageRole,
    },
    ToolCall {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamOutputItemEnd {
    Message {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
    },
    ToolCall {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        name: String,
        arguments_json_text: String,
    },
}
