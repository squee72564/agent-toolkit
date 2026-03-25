use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Shared request controls owned by the OpenAI-compatible family codec.
///
/// These fields are not semantic request intent. They are the narrow set of
/// controls the repository treats as shared across the targeted
/// OpenAI-compatible Responses surface used for OpenAI and OpenRouter.
///
/// Validation and encoding for this layer live in the OpenAI-compatible family
/// codec rather than on [`crate::TaskRequest`] or in provider-specific
/// refinements.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiCompatibleOptions {
    /// Controls whether the provider may execute tool calls in parallel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// Family-scoped reasoning controls forwarded when supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<OpenAiCompatibleReasoning>,
    /// Shared temperature control for OpenAI-compatible providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Shared nucleus sampling control for OpenAI-compatible providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Shared output token budget control for OpenAI-compatible providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

/// OpenAI-compatible reasoning controls for GPT-5 and o-series models.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiCompatibleReasoning {
    /// Reasoning effort level requested from the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<OpenAiCompatibleReasoningEffort>,
    /// Reasoning summary detail level requested from the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<OpenAiCompatibleReasoningSummary>,
}

/// OpenAI-compatible reasoning effort levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiCompatibleReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

/// OpenAI-compatible reasoning summary levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiCompatibleReasoningSummary {
    Auto,
    Concise,
    Detailed,
}

/// OpenAI-specific request controls outside the shared task and family layers.
///
/// Use this type for OpenAI Responses controls that are provider-native rather
/// than part of the shared OpenAI-compatible family contract. Validation and
/// encoding for these fields live in the OpenAI provider refinement.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiOptions {
    /// Request metadata forwarded to OpenAI.
    ///
    /// This remains provider-scoped because metadata semantics are not
    /// portable across providers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Requested OpenAI Responses `service_tier`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<OpenAiServiceTier>,
    /// Whether the response should be stored by OpenAI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// OpenAI Responses `prompt_cache_key`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// OpenAI Responses `prompt_cache_retention`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<OpenAiPromptCacheRetention>,
    /// OpenAI Responses `truncation` mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<OpenAiTruncation>,
    /// Nested OpenAI Responses `text` controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<OpenAiTextOptions>,
    /// OpenAI Responses `safety_identifier`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    /// OpenAI Responses `previous_response_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    /// OpenAI Responses `top_logprobs` integer (accepted range: 0..=20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    /// OpenAI Responses `max_tool_calls` integer (must be > 0 when set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,
}

/// OpenAI service tier selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenAiServiceTier {
    Auto,
    Default,
    Flex,
    Scale,
    Priority,
}

/// OpenAI prompt-cache retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenAiPromptCacheRetention {
    InMemory,
    #[serde(rename = "24h")]
    TwentyFourHours,
}

/// OpenAI truncation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiTruncation {
    Auto,
    Disabled,
}

/// OpenAI nested `text` request controls.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiTextOptions {
    /// Controls response verbosity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<OpenAiTextVerbosity>,
}

/// OpenAI text verbosity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiTextVerbosity {
    Low,
    Medium,
    High,
}
