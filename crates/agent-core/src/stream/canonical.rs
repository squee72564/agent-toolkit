use serde::{Deserialize, Serialize};

use crate::stream::item::{StreamOutputItemEnd, StreamOutputItemStart};
use crate::stream::raw::ProviderRawStreamEvent;
use crate::types::{FinishReason, Usage};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalStreamEnvelope {
    pub raw: ProviderRawStreamEvent,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canonical: Vec<CanonicalStreamEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanonicalStreamEvent {
    ResponseStarted {
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
    },
    OutputItemStarted {
        output_index: u32,
        item: StreamOutputItemStart,
    },
    TextDelta {
        output_index: u32,
        content_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        delta: String,
    },
    ToolCallArgumentsDelta {
        output_index: u32,
        tool_call_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        delta: String,
    },
    OutputItemCompleted {
        output_index: u32,
        item: StreamOutputItemEnd,
    },
    UsageUpdated {
        usage: Usage,
    },
    Completed {
        finish_reason: FinishReason,
    },
    Failed {
        message: String,
    },
}
