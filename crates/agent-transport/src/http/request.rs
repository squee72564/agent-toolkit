use std::time::Duration;

use agent_core::{AdapterContext, PlatformConfig};
use bytes::Bytes;
use reqwest::{
    Method, StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde_json::Value;

use crate::http::sse::SseLimits;

/// Metadata captured from an HTTP response before the body is decoded.
#[derive(Debug, Clone)]
pub struct HttpResponseHead {
    /// HTTP status code returned by the server.
    pub status: StatusCode,
    /// Response headers returned by the server.
    pub headers: HeaderMap,
    /// Request identifier extracted from the configured request-id response header, if present.
    pub request_id: Option<String>,
}

/// JSON response returned by the transport.
#[derive(Debug, Clone)]
pub struct HttpJsonResponse {
    /// Response status and headers.
    pub head: HttpResponseHead,
    /// Parsed JSON body.
    pub body: Value,
}

/// Byte response returned by the transport.
#[derive(Debug, Clone)]
pub struct HttpBytesResponse {
    /// Response status and headers.
    pub head: HttpResponseHead,
    /// Raw response body bytes.
    pub body: Bytes,
}

/// Request body variants supported by [`HttpTransport`](crate::http::HttpTransport).
#[derive(Debug, Clone)]
pub enum HttpRequestBody {
    /// Sends no request body.
    None,
    /// Sends a JSON payload and sets `content-type: application/json`.
    Json(Bytes),
    /// Sends an arbitrary byte payload with an optional explicit content type.
    Bytes {
        /// Value to send as the `content-type` header.
        content_type: Option<HeaderValue>,
        /// Raw request body bytes.
        body: Bytes,
    },
}

/// Controls how the transport should decode the response body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpResponseMode {
    /// Parse the response body as JSON.
    Json,
    /// Parse the response body as server-sent events.
    Sse,
    /// Return the response body as raw bytes.
    Bytes,
}

/// Decoded response returned by [`HttpTransport::send`](crate::http::HttpTransport::send).
#[derive(Debug)]
pub enum HttpResponse {
    /// JSON response mode output.
    Json(HttpJsonResponse),
    /// SSE response mode output.
    Sse(Box<crate::http::HttpSseResponse>),
    /// Raw bytes response mode output.
    Bytes(HttpBytesResponse),
}

/// Fully specified request for [`HttpTransport::send`](crate::http::HttpTransport::send).
pub struct HttpSendRequest<'a> {
    /// Platform-level transport configuration, including default headers and auth style.
    pub platform: &'a PlatformConfig,
    /// HTTP method to issue.
    pub method: Method,
    /// Absolute request URL.
    pub url: &'a str,
    /// Request body to send.
    pub body: HttpRequestBody,
    /// Per-call adapter context used to build auth and metadata headers.
    pub ctx: &'a AdapterContext,
    /// Per-call request overrides.
    pub options: HttpRequestOptions,
    /// Desired response decoding mode.
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

/// Header configuration derived from platform defaults and adapter metadata.
pub struct HeaderConfig {
    /// Headers that should be attached to the outbound request.
    pub headers: HeaderMap,
    /// Response header name used to extract a request identifier.
    pub request_id_header: HeaderName,
}

/// Per-request overrides for accept headers, validation, and timeout behavior.
#[derive(Debug, Clone, Default)]
pub struct HttpRequestOptions {
    /// Optional `accept` header value.
    pub accept: Option<HeaderValue>,
    /// Expected response content type, compared ignoring parameters such as `charset`.
    pub expected_content_type: Option<String>,
    /// Overrides the transport default request timeout.
    pub request_timeout: Option<Duration>,
    /// Overrides the timeout used while waiting for SSE response headers.
    pub stream_setup_timeout: Option<Duration>,
    /// Overrides the timeout between SSE chunks, including the first body bytes.
    pub stream_idle_timeout: Option<Duration>,
    /// Overrides the transport default SSE parser limits.
    pub sse_limits: Option<SseLimits>,
    /// Allows non-success HTTP statuses to be returned in JSON mode instead of producing
    /// [`TransportError::Status`](crate::http::TransportError::Status).
    pub allow_error_status: bool,
}

impl HttpRequestOptions {
    /// Sets the `accept` header sent for the request.
    pub fn with_accept(mut self, accept: HeaderValue) -> Self {
        self.accept = Some(accept);
        self
    }

    /// Requires the response `content-type` to match `expected`.
    pub fn with_expected_content_type(mut self, expected: impl Into<String>) -> Self {
        self.expected_content_type = Some(expected.into());
        self
    }

    /// Overrides the request timeout for this call.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Overrides the timeout used while waiting for SSE response headers.
    pub fn with_stream_setup_timeout(mut self, timeout: Duration) -> Self {
        self.stream_setup_timeout = Some(timeout);
        self
    }

    /// Overrides the timeout between SSE chunks once the request is in streaming mode.
    pub fn with_stream_idle_timeout(mut self, timeout: Duration) -> Self {
        self.stream_idle_timeout = Some(timeout);
        self
    }

    /// Overrides the SSE parser limits for this request.
    pub fn with_sse_limits(mut self, limits: SseLimits) -> Self {
        self.sse_limits = Some(limits);
        self
    }

    /// Controls whether JSON mode should preserve non-success statuses.
    pub fn with_allow_error_status(mut self, allow_error_status: bool) -> Self {
        self.allow_error_status = allow_error_status;
        self
    }

    /// Returns the default options used for JSON requests.
    pub fn json_defaults() -> Self {
        Self::default()
    }

    /// Returns defaults appropriate for SSE requests.
    pub fn sse_defaults() -> Self {
        Self::default()
            .with_accept(HeaderValue::from_static("text/event-stream"))
            .with_expected_content_type("text/event-stream")
    }
}
