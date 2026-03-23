use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProviderFamilyId, ProviderKind};

/// Shared request controls owned by the OpenAI-compatible family codec.
///
/// These fields are not semantic request intent. They are family-scoped
/// controls that can be encoded consistently for the OpenAI-compatible request
/// surface used by this repository.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiCompatibleOptions {
    /// Controls whether the provider may execute tool calls in parallel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// Family-scoped reasoning controls forwarded when supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
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

/// Shared request controls owned by the Anthropic family codec.
///
/// Anthropic-specific family controls stay here rather than on
/// [`crate::TaskRequest`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicFamilyOptions {
    /// Family-scoped thinking controls forwarded when supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Value>,
}

/// OpenAI-specific request controls outside the shared task and family layers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiOptions {
    /// Request metadata forwarded to OpenAI.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Requested OpenAI service tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// Whether the response should be stored by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// Provider cache-bucketing key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// Cache retention policy for prompt cache entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<OpenAiPromptCacheRetention>,
    /// Context overflow handling mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<OpenAiTruncation>,
    /// Nested provider-specific text controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<OpenAiTextOptions>,
    /// Abuse and safety correlation identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
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

/// Anthropic-specific request controls outside the shared task and family layers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicOptions {
    /// Provider-specific temperature control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Provider-specific nucleus sampling control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Provider-specific token budget control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Provider-specific top-k control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Provider-specific stop sequences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    /// Provider-specific narrow metadata identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_user_id: Option<String>,
    /// Provider-specific output shaping controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<Value>,
    /// Requested Anthropic service tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<AnthropicServiceTier>,
    /// Provider-specific tool-choice overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<AnthropicToolChoiceOptions>,
    /// Provider-specific inference geography controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<Value>,
}

/// Anthropic service-tier selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnthropicServiceTier {
    Auto,
    StandardOnly,
}

/// Anthropic nested `tool_choice` request controls.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicToolChoiceOptions {
    /// Controls whether Anthropic may execute tool calls in parallel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_parallel_tool_use: Option<bool>,
}

/// OpenRouter-specific request controls outside the shared task and family layers.
///
/// This type covers both route-backed `/responses` controls and the approved
/// parameter-doc-backed fields intentionally supported by the repo:
/// `max_tokens`, `stop`, `seed`, `logit_bias`, and `logprobs`.
/// Non-doc-backed `route` and `debug` fields remain intentionally absent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterOptions {
    /// Router fallback models appended after the selected primary model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_models: Vec<String>,
    /// Router provider selection preferences encoded as wire `provider`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_preferences: Option<Value>,
    /// OpenRouter plugin configuration objects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<Value>,
    /// Request metadata forwarded to OpenRouter.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Provider-specific top-k sampling control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Provider-specific output log-probability count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,
    /// Provider-specific maximum token budget from parameter docs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Provider-specific stop sequences from parameter docs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// Provider-specific deterministic seed from parameter docs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// Provider-specific token-bias map from parameter docs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub logit_bias: BTreeMap<String, i32>,
    /// Whether OpenRouter should return token log probabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// Provider-specific frequency penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Provider-specific presence penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Abuse/account attribution identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Observability session identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// OpenRouter tracing payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Value>,
    /// Nested provider-specific text controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<OpenRouterTextOptions>,
    /// Output modalities requested from OpenRouter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
    /// Provider-specific image generation configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_config: Option<Value>,
}

/// OpenRouter nested `text` request controls.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterTextOptions {
    /// Controls response verbosity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<OpenRouterTextVerbosity>,
}

/// OpenRouter text verbosity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenRouterTextVerbosity {
    Low,
    Medium,
    High,
    Max,
}

/// Family-scoped native request controls.
///
/// Use this layer for controls shared by one provider family but not portable
/// enough for [`crate::TaskRequest`]. Validation and encoding belong to the
/// family codec that owns the fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum FamilyOptions {
    OpenAiCompatible(OpenAiCompatibleOptions),
    Anthropic(AnthropicFamilyOptions),
}

impl FamilyOptions {
    /// Returns the family targeted by this option layer.
    pub fn family_id(&self) -> ProviderFamilyId {
        match self {
            Self::OpenAiCompatible(_) => ProviderFamilyId::OpenAiCompatible,
            Self::Anthropic(_) => ProviderFamilyId::Anthropic,
        }
    }
}

/// Provider-scoped native request controls.
///
/// Use this layer for provider-native or router-native controls that should not
/// be modeled as semantic request fields. Direct-provider runtime helpers accept
/// these typed values via `create_with_*_options(...)` alongside semantic
/// [`crate::TaskRequest`] input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum ProviderOptions {
    OpenAi(OpenAiOptions),
    Anthropic(AnthropicOptions),
    OpenRouter(Box<OpenRouterOptions>),
}

impl ProviderOptions {
    /// Returns the provider kind targeted by this option layer.
    pub fn provider_kind(&self) -> ProviderKind {
        match self {
            Self::OpenAi(_) => ProviderKind::OpenAi,
            Self::Anthropic(_) => ProviderKind::Anthropic,
            Self::OpenRouter(_) => ProviderKind::OpenRouter,
        }
    }
}

/// Layered family-scoped and provider-scoped native request controls.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeOptions {
    /// Family-scoped controls consumed by the provider family codec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<FamilyOptions>,
    /// Provider-scoped controls consumed by the concrete provider refinement layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderOptions>,
}
