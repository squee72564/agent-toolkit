use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProviderFamilyId, ProviderKind};

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
/// [`crate::TaskRequest`]. Validation and encoding for this layer live in the
/// Anthropic family codec.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicFamilyOptions {
    /// Family-scoped thinking controls forwarded when supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Value>,
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
    pub service_tier: Option<String>,
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
///
/// Use this type for Anthropic Messages controls that are provider-native
/// rather than semantic request intent. Validation and encoding for these
/// fields live in the Anthropic provider refinement.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicOptions {
    /// Anthropic Messages `temperature`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Anthropic Messages `top_p`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Anthropic Messages `max_tokens`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Anthropic Messages `top_k`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Anthropic Messages `stop_sequences`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    /// Anthropic Messages `metadata.user_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_user_id: Option<String>,
    /// Anthropic Messages `output_config`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<Value>,
    /// Anthropic Messages `service_tier`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<AnthropicServiceTier>,
    /// Anthropic Messages nested `tool_choice` overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<AnthropicToolChoiceOptions>,
    /// Anthropic Messages `inference_geo`.
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
    /// Anthropic `tool_choice.disable_parallel_tool_use`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_parallel_tool_use: Option<bool>,
}

/// OpenRouter-specific request controls outside the shared task and family layers.
///
/// This type covers both route-backed `/responses` controls and the approved
/// parameter-doc-backed Tier 2 fields intentionally supported by the
/// repository: `max_tokens`, `stop`, `seed`, `logit_bias`, and `logprobs`.
///
/// Validation and encoding for these fields live in the OpenRouter provider
/// refinement. Non-doc-backed `route` and `debug` fields remain intentionally
/// absent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterOptions {
    /// Additional OpenRouter fallback models appended after the selected primary model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_models: Vec<String>,
    /// OpenRouter provider routing preferences encoded as wire `provider`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_preferences: Option<Value>,
    /// OpenRouter `plugins`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<Value>,
    /// OpenRouter request metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// OpenRouter `top_k`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// OpenRouter `top_logprobs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,
    /// OpenRouter `max_tokens` accepted from the broader parameter docs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// OpenRouter `stop` accepted from the broader parameter docs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// OpenRouter `seed` accepted from the broader parameter docs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// OpenRouter `logit_bias` accepted from the broader parameter docs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub logit_bias: BTreeMap<String, i32>,
    /// OpenRouter `logprobs` accepted from the broader parameter docs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// OpenRouter `frequency_penalty`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// OpenRouter `presence_penalty`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// OpenRouter `user`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// OpenRouter `session_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// OpenRouter `trace`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Value>,
    /// Nested OpenRouter `text` controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<OpenRouterTextOptions>,
    /// OpenRouter `modalities`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
    /// OpenRouter `image_config`.
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
/// family codec that owns the fields, and each enum variant mirrors the family
/// boundary used by request planning and adapter composition.
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
/// [`crate::TaskRequest`] input, and provider refinements own validation plus
/// last-mile request encoding for each variant.
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
