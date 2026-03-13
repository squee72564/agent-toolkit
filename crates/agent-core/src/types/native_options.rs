use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProviderFamilyId, ProviderKind};

/// Shared controls for the OpenAI-compatible provider family.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiCompatibleOptions {
    /// Controls whether the provider may execute tool calls in parallel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// Family-scoped reasoning controls forwarded when supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
}

/// Shared controls for the Anthropic provider family.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicFamilyOptions {
    /// Family-scoped thinking controls forwarded when supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Value>,
}

/// OpenAI-specific controls outside the shared task and family layers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiOptions {
    /// Requested OpenAI service tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// Whether the response should be stored by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
}

/// Anthropic-specific controls outside the shared task and family layers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicOptions {
    /// Provider-specific top-k control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
}

/// OpenRouter-specific controls outside the shared task and family layers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterOptions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_preferences: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<Value>,
}

impl OpenRouterOptions {
    /// Creates an empty provider-specific options value.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the OpenRouter route selector.
    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }
}

/// Family-scoped native request controls.
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
    /// Provider-scoped controls consumed by the concrete provider overlay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderOptions>,
}
