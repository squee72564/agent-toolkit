use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
