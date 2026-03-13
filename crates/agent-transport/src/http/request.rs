use agent_core::{AuthCredentials, PlatformConfig, ResolvedTransportOptions};
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
pub enum TransportResponseFraming {
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
pub struct TransportExecutionInput<'a> {
    /// Platform-level transport configuration, including default headers and auth style.
    pub platform: &'a PlatformConfig,
    /// Optional auth credentials used for transport-owned auth placement.
    pub auth: Option<&'a AuthCredentials>,
    /// HTTP method to issue.
    pub method: Method,
    /// Absolute request URL.
    pub url: &'a str,
    /// Request body to send.
    pub body: HttpRequestBody,
    /// Transport response framing selected by provider planning.
    pub response_framing: TransportResponseFraming,
    /// Adapter-owned protocol-level request/response hints.
    pub options: HttpRequestOptions,
    /// Runtime-resolved transport options for this attempt.
    pub transport: ResolvedTransportOptions,
    /// Adapter-owned dynamic provider headers.
    pub provider_headers: HeaderMap,
}

pub type HttpSendRequest<'a> = TransportExecutionInput<'a>;

pub(crate) struct RequestExecution<'a> {
    pub platform: &'a PlatformConfig,
    pub auth: Option<&'a AuthCredentials>,
    pub method: Method,
    pub url: &'a str,
    pub body: &'a HttpRequestBody,
    pub response_framing: TransportResponseFraming,
    pub options: &'a HttpRequestOptions,
    pub transport: &'a ResolvedTransportOptions,
    pub provider_headers: &'a HeaderMap,
}

/// Header configuration derived from explicit platform, caller, adapter, and auth layers.
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
