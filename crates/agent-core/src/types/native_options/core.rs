use serde::{Deserialize, Serialize};

use crate::{
    AnthropicFamilyOptions, AnthropicOptions, OpenAiCompatibleOptions, OpenAiOptions,
    OpenRouterOptions, ProviderFamilyId, ProviderKind,
};

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
