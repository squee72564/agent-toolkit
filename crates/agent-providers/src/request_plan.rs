//! Provider request plans consumed by the transport/runtime layers.

use agent_core::RuntimeWarning;
use agent_transport::HttpRequestOptions;
use serde_json::Value;

/// Transport mode required to execute a provider request plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTransportKind {
    /// Execute the request as a standard JSON HTTP exchange.
    HttpJson,
    /// Execute the request as an SSE stream.
    HttpSse,
}

/// Expected shape of the provider response body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderResponseKind {
    /// Expect a complete JSON response body.
    JsonBody,
    /// Expect provider-native streaming events that must be projected.
    RawProviderStream,
}

/// Provider-specific execution contract returned by an adapter.
#[derive(Debug, Clone)]
pub struct ProviderRequestPlan {
    /// Serialized provider request body.
    pub body: Value,
    /// Non-fatal warnings produced while planning the request.
    pub warnings: Vec<RuntimeWarning>,
    /// Transport mode required for the request.
    pub transport_kind: ProviderTransportKind,
    /// Expected response mode for the request.
    pub response_kind: ProviderResponseKind,
    /// Optional endpoint path override relative to the platform base URL.
    pub endpoint_path_override: Option<String>,
    /// Transport-level request options such as timeouts and SSE limits.
    pub request_options: HttpRequestOptions,
}
