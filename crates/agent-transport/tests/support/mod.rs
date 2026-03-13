use std::error::Error;
use std::time::Duration;

use agent_core::types::{
    AuthStyle, PlatformConfig, ProtocolKind, ResolvedTransportOptions, TransportTimeoutOverrides,
};
use agent_transport::{HttpTransport, RetryPolicy};
use reqwest::header::{HeaderMap, HeaderName};
use serde::Serialize;

pub mod http_server;

pub type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

pub fn default_platform(auth_style: AuthStyle) -> PlatformConfig {
    PlatformConfig {
        protocol: ProtocolKind::OpenAI,
        base_url: "http://localhost".to_string(),
        auth_style,
        request_id_header: HeaderName::from_static("x-request-id"),
        default_headers: HeaderMap::new(),
    }
}

pub fn default_transport(retry_policy: RetryPolicy) -> HttpTransport {
    HttpTransport::builder(reqwest::Client::new())
        .retry_policy(retry_policy)
        .request_timeout(Duration::from_secs(2))
        .stream_timeout(Duration::from_secs(2))
        .build()
}

pub fn default_resolved_transport(retry_policy: RetryPolicy) -> ResolvedTransportOptions {
    ResolvedTransportOptions {
        request_id_header_override: None,
        route_extra_headers: Default::default(),
        attempt_extra_headers: Default::default(),
        timeouts: TransportTimeoutOverrides {
            request_timeout: Some(Duration::from_secs(2)),
            stream_setup_timeout: Some(Duration::from_secs(2)),
            stream_idle_timeout: Some(Duration::from_secs(2)),
        },
        retry_policy,
    }
}

#[derive(Serialize)]
pub struct ExampleBody<'a> {
    pub msg: &'a str,
}
