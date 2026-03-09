use std::collections::BTreeMap;
use std::error::Error;
use std::time::Duration;

use agent_core::types::{AdapterContext, AuthStyle, PlatformConfig, ProtocolKind};
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

pub fn empty_context() -> AdapterContext {
    AdapterContext {
        metadata: BTreeMap::new(),
        auth_token: None,
    }
}

#[derive(Serialize)]
pub struct ExampleBody<'a> {
    pub msg: &'a str,
}
