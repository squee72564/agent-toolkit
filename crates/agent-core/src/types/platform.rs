use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderId {
    OpenAi,
    Anthropic,
    OpenRouter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterContext {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<AuthCredentials>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AuthCredentials {
    Token(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStyle {
    Bearer,
    ApiKeyHeader(reqwest::header::HeaderName),
    Basic,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformConfig {
    pub protocol: ProtocolKind,
    pub base_url: String,
    pub auth_style: AuthStyle,
    pub request_id_header: reqwest::header::HeaderName,
    pub default_headers: reqwest::header::HeaderMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolKind {
    OpenAI,
    Anthropic,
}
