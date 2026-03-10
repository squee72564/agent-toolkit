use serde::{Deserialize, Serialize};

use crate::types::MessageRole;

/// Descriptor for a canonical stream item when it first appears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamOutputItemStart {
    /// A streamed message item.
    Message {
        /// Provider item id when available.
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        /// Message role associated with the item.
        role: MessageRole,
    },
    /// A streamed tool call item.
    ToolCall {
        /// Provider item id when available.
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        /// Provider tool call id when available.
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        /// Tool name.
        name: String,
    },
}

/// Descriptor for a canonical stream item once the provider marks it complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamOutputItemEnd {
    /// A completed message item.
    Message {
        /// Provider item id when available.
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
    },
    /// A completed tool call item.
    ToolCall {
        /// Provider item id when available.
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        /// Provider tool call id when available.
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        /// Tool name.
        name: String,
        /// Final JSON argument text accumulated for the call.
        arguments_json_text: String,
    },
}
