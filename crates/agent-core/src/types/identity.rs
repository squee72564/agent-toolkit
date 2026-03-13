use std::borrow::Borrow;

use reqwest::header::{HeaderMap, HeaderName};
use serde::{Deserialize, Serialize};

use super::platform::{AuthStyle, ProtocolKind};

/// Identifier for a shared provider protocol family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderFamilyId {
    /// OpenAI-compatible request/response contracts.
    OpenAiCompatible,
    /// Anthropic message APIs.
    Anthropic,
}

/// Identifier for a concrete adapter and provider overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    /// OpenAI-hosted APIs.
    OpenAi,
    /// Anthropic-hosted APIs.
    Anthropic,
    /// OpenRouter-hosted APIs.
    OpenRouter,
    /// Generic self-hosted OpenAI-compatible endpoints.
    GenericOpenAiCompatible,
}

/// REFACTOR-SHIM: preserve the old provider identity name while the runtime migrates.
pub type ProviderId = ProviderKind;

/// Identifier for one registered runtime destination instance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderInstanceId(String);

impl ProviderInstanceId {
    /// Creates a new provider instance identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Borrow<str> for ProviderInstanceId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl From<String> for ProviderInstanceId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ProviderInstanceId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for ProviderInstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<ProviderKind> for ProviderInstanceId {
    fn from(kind: ProviderKind) -> Self {
        match kind {
            ProviderKind::OpenAi => Self::new("openai-default"),
            ProviderKind::Anthropic => Self::new("anthropic-default"),
            ProviderKind::OpenRouter => Self::new("openrouter-default"),
            ProviderKind::GenericOpenAiCompatible => Self::new("generic-openai-compatible-default"),
        }
    }
}

/// Adapter-owned static provider metadata keyed by [`ProviderKind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDescriptor {
    /// Concrete provider kind.
    pub kind: ProviderKind,
    /// Shared protocol family.
    pub family: ProviderFamilyId,
    /// Wire protocol expected by the provider adapter and transport.
    pub protocol: ProtocolKind,
    /// Default API base URL for this provider kind.
    pub default_base_url: &'static str,
    /// Default endpoint path for request execution.
    pub endpoint_path: &'static str,
    /// Default auth placement strategy.
    pub default_auth_style: AuthStyle,
    /// Default response header name used for request-id extraction.
    pub default_request_id_header: HeaderName,
    /// Default headers attached to every request for this provider kind.
    pub default_headers: HeaderMap,
}
