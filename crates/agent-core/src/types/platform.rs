use serde::{Deserialize, Serialize};

/// Authentication material supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AuthCredentials {
    /// Raw token value interpreted according to [`AuthStyle`].
    Token(String),
}

/// Strategy the HTTP transport should use to place [`AuthCredentials`] on outbound requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStyle {
    /// Send the token as an `Authorization: Bearer ...` header.
    Bearer,
    /// Send the token in a provider-specific header.
    ApiKeyHeader(reqwest::header::HeaderName),
    /// Base64-encode the token and send it as HTTP basic auth credentials.
    Basic,
    /// Do not attach authentication headers.
    None,
}

/// Provider platform configuration used by the transport layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformConfig {
    /// Wire protocol expected by the provider adapter and transport.
    pub protocol: ProtocolKind,
    /// Base URL for the target API.
    pub base_url: String,
    /// Authentication strategy for outbound requests.
    pub auth_style: AuthStyle,
    /// Header name from which the transport reads request ids in responses by default.
    pub request_id_header: reqwest::header::HeaderName,
    /// Static headers included on every outbound request.
    pub default_headers: reqwest::header::HeaderMap,
}

/// Request and response dialect understood by a provider endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolKind {
    /// OpenAI-compatible request and response shapes.
    OpenAI,
    /// Anthropic Messages API request and response shapes.
    Anthropic,
}
