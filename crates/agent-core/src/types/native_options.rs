use std::{collections::BTreeMap, num::NonZeroU32};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnthropicThinkingDisplay {
    Summarized,
    Omitted,
}

/// Anthropic extended-thinking budget in tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AnthropicThinkingBudget(NonZeroU32);

impl AnthropicThinkingBudget {
    /// Creates a non-zero budget value.
    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }

    /// Returns the underlying token count.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Anthropic Messages `thinking` configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicThinking {
    Disabled,
    Enabled {
        budget_tokens: AnthropicThinkingBudget,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<AnthropicThinkingDisplay>,
    },
    Adaptive {
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<AnthropicThinkingDisplay>,
    },
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
    pub thinking: Option<AnthropicThinking>,
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
    pub output_config: Option<AnthropicOutputConfig>,
    /// Anthropic Messages `service_tier`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<AnthropicServiceTier>,
    /// Anthropic Messages nested `tool_choice` overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<AnthropicToolChoiceOptions>,
    /// Anthropic Messages `inference_geo`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,
}

/// Anthropic `output_config` settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicOutputConfig {
    /// Anthropic `output_config.effort`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<AnthropicOutputEffort>,
    /// Anthropic `output_config.format`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<AnthropicOutputFormat>,
}

/// Anthropic `output_config.effort` levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnthropicOutputEffort {
    Low,
    Medium,
    High,
    Max,
}

/// Anthropic structured-output format options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicOutputFormat {
    /// JSON schema describing the requested format.
    pub schema: Value,
    /// Format discriminator.
    #[serde(rename = "type")]
    pub format_type: AnthropicOutputFormatType,
}

/// Anthropic output format types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnthropicOutputFormatType {
    #[serde(rename = "json_schema")]
    JsonSchema,
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
    pub plugins: Vec<OpenRouterPlugin>,
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
    pub trace: Option<OpenRouterTrace>,
    /// Nested OpenRouter `text` controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<OpenRouterTextOptions>,
    /// OpenRouter `modalities`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
    /// OpenRouter `image_config`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_config: Option<OpenRouterImageConfig>,
}

/// OpenRouter trace metadata.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterTrace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
}

/// OpenRouter provider-specific image configuration.
pub type OpenRouterImageConfig = BTreeMap<String, OpenRouterImageConfigValue>;

/// OpenRouter image configuration values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenRouterImageConfigValue {
    String(String),
    Number(f64),
}

/// OpenRouter plugins enabled for a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "id")]
pub enum OpenRouterPlugin {
    #[serde(rename = "auto-router")]
    AutoRouter(OpenRouterAutoRouterPlugin),
    #[serde(rename = "moderation")]
    Moderation(OpenRouterModerationPlugin),
    #[serde(rename = "web")]
    Web(OpenRouterWebPlugin),
    #[serde(rename = "file-parser")]
    FileParser(OpenRouterFileParserPlugin),
    #[serde(rename = "response-healing")]
    ResponseHealing(OpenRouterResponseHealingPlugin),
    #[serde(rename = "context-compression")]
    ContextCompression(OpenRouterContextCompressionPlugin),
}

/// OpenRouter `auto-router` plugin settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterAutoRouterPlugin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_models: Vec<String>,
}

/// OpenRouter `moderation` plugin settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterModerationPlugin {}

/// OpenRouter `web` plugin settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterWebPlugin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<OpenRouterWebPluginEngine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_domains: Vec<String>,
}

/// OpenRouter `web` plugin engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenRouterWebPluginEngine {
    Native,
    Exa,
    Firecrawl,
    Parallel,
}

/// OpenRouter `file-parser` plugin settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterFileParserPlugin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf: Option<OpenRouterFileParserPdfOptions>,
}

/// OpenRouter `file-parser.pdf` settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterFileParserPdfOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<OpenRouterFileParserPdfEngine>,
}

/// OpenRouter `file-parser.pdf.engine` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenRouterFileParserPdfEngine {
    #[serde(rename = "mistral-ocr")]
    MistralOcr,
    #[serde(rename = "pdf-text")]
    PdfText,
    #[serde(rename = "native")]
    Native,
}

/// OpenRouter `response-healing` plugin settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterResponseHealingPlugin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// OpenRouter `context-compression` plugin settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterContextCompressionPlugin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<OpenRouterContextCompressionEngine>,
}

/// OpenRouter `context-compression.engine` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenRouterContextCompressionEngine {
    #[serde(rename = "middle-out")]
    MiddleOut,
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
