use std::{collections::BTreeMap, num::NonZeroU32};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    /// Anthropic Messages `cache_control`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<AnthropicCacheControl>,

    /// Request metadata forwarded to Anthropic.
    ///
    /// This remains provider-scoped because metadata semantics are not
    /// portable across providers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicCacheControl {
    #[serde(rename = "type")]
    pub type_: AnthropicCacheControlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<AnthropicCacheControlTTL>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AnthropicCacheControlType {
    #[serde(rename = "ephemeral")]
    Ephemeral,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AnthropicCacheControlTTL {
    #[serde(rename = "5m")]
    FiveMinute,
    #[serde(rename = "1h")]
    OneHour,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicToolChoiceOptions {
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Tool {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
        name: String,
    },
    None,
}
