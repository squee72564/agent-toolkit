use std::time::Duration;

use agent_core::{AdapterContext, PlatformConfig};
use bytes::Bytes;
use reqwest::{
    Method, StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde_json::Value;

use crate::http::sse::SseLimits;

#[derive(Debug, Clone)]
pub struct HttpResponseHead {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HttpJsonResponse {
    pub head: HttpResponseHead,
    pub body: Value,
}

#[derive(Debug, Clone)]
pub struct HttpBytesResponse {
    pub head: HttpResponseHead,
    pub body: Bytes,
}

#[derive(Debug, Clone)]
pub enum HttpRequestBody {
    None,
    Json(Bytes),
    Bytes {
        content_type: Option<HeaderValue>,
        body: Bytes,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpResponseMode {
    Json,
    Sse,
    Bytes,
}

#[derive(Debug)]
pub enum HttpResponse {
    Json(HttpJsonResponse),
    Sse(Box<crate::http::HttpSseResponse>),
    Bytes(HttpBytesResponse),
}

pub struct HttpSendRequest<'a> {
    pub platform: &'a PlatformConfig,
    pub method: Method,
    pub url: &'a str,
    pub body: HttpRequestBody,
    pub ctx: &'a AdapterContext,
    pub options: HttpRequestOptions,
    pub response_mode: HttpResponseMode,
}

pub(crate) struct RequestExecution<'a> {
    pub platform: &'a PlatformConfig,
    pub method: Method,
    pub url: &'a str,
    pub body: &'a HttpRequestBody,
    pub ctx: &'a AdapterContext,
    pub options: &'a HttpRequestOptions,
    pub response_mode: HttpResponseMode,
}

pub struct HeaderConfig {
    pub headers: HeaderMap,
    pub request_id_header: HeaderName,
}

#[derive(Debug, Clone, Default)]
pub struct HttpRequestOptions {
    pub accept: Option<HeaderValue>,
    pub expected_content_type: Option<String>,
    pub request_timeout: Option<Duration>,
    pub stream_setup_timeout: Option<Duration>,
    pub stream_idle_timeout: Option<Duration>,
    pub sse_limits: Option<SseLimits>,
}

impl HttpRequestOptions {
    pub fn with_accept(mut self, accept: HeaderValue) -> Self {
        self.accept = Some(accept);
        self
    }

    pub fn with_expected_content_type(mut self, expected: impl Into<String>) -> Self {
        self.expected_content_type = Some(expected.into());
        self
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    pub fn with_stream_setup_timeout(mut self, timeout: Duration) -> Self {
        self.stream_setup_timeout = Some(timeout);
        self
    }

    pub fn with_stream_idle_timeout(mut self, timeout: Duration) -> Self {
        self.stream_idle_timeout = Some(timeout);
        self
    }

    pub fn with_sse_limits(mut self, limits: SseLimits) -> Self {
        self.sse_limits = Some(limits);
        self
    }

    pub fn json_defaults() -> Self {
        Self::default()
    }

    pub fn sse_defaults() -> Self {
        Self::default()
            .with_accept(HeaderValue::from_static("text/event-stream"))
            .with_expected_content_type("text/event-stream")
    }
}
