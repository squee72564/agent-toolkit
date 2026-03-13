//! Provider request plans consumed by the runtime and transport boundary.

use agent_core::RuntimeWarning;
use agent_transport::HttpRequestOptions;
pub use agent_transport::TransportResponseFraming;
use reqwest::{Method, header::HeaderMap};
use serde_json::Value;

/// Family-level intermediate request plan before provider overlay refinement.
#[derive(Debug, Clone)]
pub struct EncodedFamilyRequest {
    /// Serialized provider request body.
    pub body: Value,
    /// Non-fatal warnings produced while planning the request.
    pub warnings: Vec<RuntimeWarning>,
    /// Outbound HTTP method selected by the family codec.
    pub method: Method,
    /// Transport response framing selected by the family codec.
    pub response_framing: TransportResponseFraming,
    /// Optional endpoint path override relative to the platform base URL.
    pub endpoint_path_override: Option<String>,
    /// Adapter-generated dynamic headers to forward with the request.
    pub provider_headers: HeaderMap,
    /// Closed protocol-level request/response hints for transport.
    pub request_options: HttpRequestOptions,
}

/// Final adapter-produced request contract consumed by runtime.
#[derive(Debug, Clone)]
pub struct ProviderRequestPlan {
    /// Serialized provider request body.
    pub body: Value,
    /// Non-fatal warnings produced while planning the request.
    pub warnings: Vec<RuntimeWarning>,
    /// Outbound HTTP method selected by the adapter.
    pub method: Method,
    /// Transport response framing selected by the adapter.
    pub response_framing: TransportResponseFraming,
    /// Optional endpoint path override relative to the platform base URL.
    pub endpoint_path_override: Option<String>,
    /// Adapter-generated dynamic headers to forward with the request.
    pub provider_headers: HeaderMap,
    /// Closed protocol-level request/response hints for transport.
    pub request_options: HttpRequestOptions,
}

impl From<EncodedFamilyRequest> for ProviderRequestPlan {
    fn from(value: EncodedFamilyRequest) -> Self {
        Self {
            body: value.body,
            warnings: value.warnings,
            method: value.method,
            response_framing: value.response_framing,
            endpoint_path_override: value.endpoint_path_override,
            provider_headers: value.provider_headers,
            request_options: value.request_options,
        }
    }
}
