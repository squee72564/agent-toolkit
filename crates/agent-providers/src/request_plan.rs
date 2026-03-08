use agent_core::RuntimeWarning;
use agent_transport::HttpRequestOptions;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTransportKind {
    HttpJson,
    HttpSse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderResponseKind {
    JsonBody,
    RawProviderStream,
}

#[derive(Debug, Clone)]
pub struct ProviderRequestPlan {
    pub body: Value,
    pub warnings: Vec<RuntimeWarning>,
    pub transport_kind: ProviderTransportKind,
    pub response_kind: ProviderResponseKind,
    pub endpoint_path_override: Option<String>,
    pub request_options: HttpRequestOptions,
}
