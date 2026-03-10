use serde::{Deserialize, Serialize};

use crate::stream::item::{StreamOutputItemEnd, StreamOutputItemStart};
use crate::stream::raw::ProviderRawStreamEvent;
use crate::types::{FinishReason, Usage};

/// A raw provider frame paired with the canonical events derived from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalStreamEnvelope {
    /// The original provider frame.
    pub raw: ProviderRawStreamEvent,
    /// Zero or more canonical events projected from [`Self::raw`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canonical: Vec<CanonicalStreamEvent>,
}

/// Provider-independent streaming event emitted by an adapter projector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanonicalStreamEvent {
    /// Marks the beginning of a streamed response.
    ResponseStarted {
        /// Model id if the provider exposes it at stream start.
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// Provider response id if available.
        #[serde(skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
    },
    /// Starts a new output item at the given output index.
    OutputItemStarted {
        /// Provider output slot.
        output_index: u32,
        /// Descriptor for the item being opened.
        item: StreamOutputItemStart,
    },
    /// Appends text to an in-flight message item.
    TextDelta {
        /// Provider output slot.
        output_index: u32,
        /// Provider content slot within the output item.
        content_index: u32,
        /// Provider item id when present.
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        /// Incremental text payload.
        delta: String,
    },
    /// Appends argument text to an in-flight tool call item.
    ToolCallArgumentsDelta {
        /// Provider output slot.
        output_index: u32,
        /// Provider tool-call slot used to correlate deltas.
        tool_call_index: u32,
        /// Provider item id when present.
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        /// Provider tool call id when present.
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        /// Tool name when the provider includes it on delta frames.
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        /// Incremental JSON text for the tool call arguments.
        delta: String,
    },
    /// Completes an output item and includes its final descriptor.
    OutputItemCompleted {
        /// Provider output slot.
        output_index: u32,
        /// Descriptor for the completed item.
        item: StreamOutputItemEnd,
    },
    /// Replaces the latest usage totals observed on the stream.
    UsageUpdated {
        /// Current usage snapshot.
        usage: Usage,
    },
    /// Marks a successful terminal stream state.
    Completed {
        /// Canonical finish reason.
        finish_reason: FinishReason,
    },
    /// Marks a terminal stream failure.
    Failed {
        /// Human-readable failure detail.
        message: String,
    },
}
