use std::time::Duration;

use agent_core::{
    AuthCredentials, PlatformConfig, ResolvedTransportOptions, RetryPolicy,
    TransportTimeoutOverrides,
};
use bytes::{Bytes, BytesMut};
use reqwest::{
    Method,
    header::{CONTENT_TYPE, HeaderValue, InvalidHeaderName, InvalidHeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

use crate::http::builder::HttpTransportBuilder;
use crate::http::headers::build_header_config;
use crate::http::request::{
    HeaderConfig, HttpBytesResponse, HttpJsonResponse, HttpRequestBody, HttpRequestOptions,
    HttpResponse, HttpResponseHead, HttpSendRequest, RequestExecution, TransportResponseFraming,
};
use crate::http::response::{build_response_head, content_type_matches};
use crate::http::sse::{HttpSseResponse, HttpSseStream, PendingSseEvent, SseLimits};

/// Errors produced while building requests or decoding responses.
#[derive(Debug, Error)]
pub enum TransportError {
    /// A header name derived from explicit transport inputs was invalid.
    #[error("invalid header name")]
    InvalidHeaderName,
    /// A header value derived from explicit transport inputs was invalid.
    #[error("invalid header value")]
    InvalidHeaderValue,
    /// JSON serialization or deserialization failed.
    #[error("serialization error")]
    Serialization,
    /// A timeout fired while a request or stream was in progress.
    #[error("request timed out during {stage}")]
    Timeout { stage: TimeoutStage },
    /// The server returned a non-success status that the request mode did not allow.
    #[error("unexpected HTTP status {head:?}")]
    Status {
        /// Response status and headers for the failed request.
        head: Box<crate::http::HttpResponseHead>,
    },
    /// The response content type did not match the expected value.
    #[error("unexpected response content-type: expected {expected}, got {actual:?}")]
    ContentTypeMismatch {
        /// Expected media type.
        expected: String,
        /// Actual `content-type` value, if one was present and valid UTF-8.
        actual: Option<String>,
        /// Response status and headers.
        head: Box<crate::http::HttpResponseHead>,
    },
    /// An SSE stream ended after it had started but before a clean termination point.
    #[error("stream terminated unexpectedly ({reason}): {message}")]
    StreamTerminated {
        /// Reason category for the termination.
        reason: StreamTerminationReason,
        /// Lower-level error details.
        message: String,
        /// Response status and headers for the stream.
        head: Box<crate::http::HttpResponseHead>,
    },
    /// The SSE payload was not valid according to the event stream format.
    #[error("invalid SSE stream: {0}")]
    SseParse(String),
    /// An SSE parser limit was exceeded.
    #[error("{kind} exceeded limit: {size} > {max}")]
    SseLimit {
        /// Name of the limit that fired.
        kind: &'static str,
        /// Observed size.
        size: usize,
        /// Configured maximum size.
        max: usize,
    },
    /// `reqwest` returned an error that was not normalized into another transport error.
    #[error("request error: {0}")]
    Request(reqwest::Error),
}

/// Stages used to classify timeout failures.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutStage {
    /// The overall request timed out before a response was received.
    #[error("request")]
    Request,
    /// Waiting for SSE response headers timed out.
    #[error("stream setup")]
    StreamSetup,
    /// Waiting for the first body bytes of an SSE stream timed out.
    #[error("first byte")]
    FirstByte,
    /// Waiting for additional body bytes of an SSE stream timed out.
    #[error("stream idle")]
    StreamIdle,
}

/// Categories of abnormal SSE stream termination.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum StreamTerminationReason {
    /// The underlying connection closed unexpectedly.
    #[error("disconnect")]
    Disconnect,
    /// The stream exceeded an idle timeout.
    #[error("idle timeout")]
    IdleTimeout,
    /// The stream ended in a protocol-invalid state.
    #[error("protocol")]
    Protocol,
}

impl From<reqwest::Error> for TransportError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return TransportError::Timeout {
                stage: TimeoutStage::Request,
            };
        }

        TransportError::Request(err)
    }
}

impl From<InvalidHeaderName> for TransportError {
    fn from(_: InvalidHeaderName) -> Self {
        TransportError::InvalidHeaderName
    }
}

impl From<InvalidHeaderValue> for TransportError {
    fn from(_: InvalidHeaderValue) -> Self {
        TransportError::InvalidHeaderValue
    }
}

/// HTTP transport with retry, timeout, and SSE support.
#[derive(Debug, Clone)]
pub struct HttpTransport {
    pub(crate) client: reqwest::Client,
    pub(crate) retry_policy: RetryPolicy,
    pub(crate) request_timeout: Duration,
    pub(crate) stream_timeout: Duration,
    pub(crate) sse_limits: SseLimits,
}

impl HttpTransport {
    /// Starts building a transport around an existing `reqwest` client.
    pub fn builder(client: reqwest::Client) -> HttpTransportBuilder {
        HttpTransportBuilder {
            client,
            retry_policy: RetryPolicy::default(),
            request_timeout: Duration::from_secs(30),
            stream_timeout: Duration::from_secs(30),
            sse_limits: SseLimits::default(),
        }
    }

    /// Builds request headers from explicit platform, caller, adapter, and auth layers.
    pub fn build_header_config(
        &self,
        platform: &PlatformConfig,
        auth: Option<&AuthCredentials>,
        transport: &ResolvedTransportOptions,
        provider_headers: &reqwest::header::HeaderMap,
    ) -> Result<HeaderConfig, TransportError> {
        build_header_config(platform, auth, transport, provider_headers)
    }

    /// Returns the transport-level default retry policy.
    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.retry_policy
    }

    /// Returns the transport-level default non-streaming request timeout.
    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Returns the transport-level default stream setup and idle timeout.
    pub fn stream_timeout(&self) -> Duration {
        self.stream_timeout
    }

    /// Sends a request and decodes the response according to `response_framing`.
    ///
    /// Retries are applied only before a response body is handed to the caller. For JSON mode,
    /// non-success statuses can be preserved by setting
    /// [`HttpRequestOptions::allow_error_status`](crate::http::HttpRequestOptions::allow_error_status).
    pub async fn send(&self, request: HttpSendRequest<'_>) -> Result<HttpResponse, TransportError> {
        let HttpSendRequest {
            platform,
            auth,
            method,
            url,
            body,
            response_framing,
            options,
            transport,
            provider_headers,
        } = request;
        let (mut response, header_config) = self
            .send_request_with_retry(RequestExecution {
                platform,
                auth,
                method,
                url,
                body: &body,
                response_framing,
                options: &options,
                transport: &transport,
                provider_headers: &provider_headers,
            })
            .await?;

        let head = build_response_head(&response, &header_config);
        if !(head.status.is_success()
            || matches!(response_framing, TransportResponseFraming::Json)
                && options.allow_error_status)
        {
            return Err(TransportError::Status {
                head: Box::new(head),
            });
        }

        if let Some(expected) = options.expected_content_type.as_deref()
            && !content_type_matches(&head.headers, expected)
        {
            let actual = head
                .headers
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);

            return Err(TransportError::ContentTypeMismatch {
                expected: expected.to_string(),
                actual,
                head: Box::new(head),
            });
        }

        match response_framing {
            TransportResponseFraming::Json => {
                let body = response.json::<Value>().await?;
                Ok(HttpResponse::Json(HttpJsonResponse { head, body }))
            }
            TransportResponseFraming::Bytes => {
                let body = response.bytes().await?;
                Ok(HttpResponse::Bytes(HttpBytesResponse { head, body }))
            }
            TransportResponseFraming::Sse => {
                let mut buffer = BytesMut::new();
                let idle_timeout = transport.timeouts.stream_idle_timeout;
                let first_chunk = self
                    .read_stream_chunk(&mut response, &head, idle_timeout, TimeoutStage::FirstByte)
                    .await?;

                let received_any_bytes = first_chunk.is_some();
                if let Some(chunk) = first_chunk {
                    buffer.extend_from_slice(&chunk);
                }

                let stream = HttpSseStream {
                    head: head.clone(),
                    response,
                    buffer,
                    buffer_offset: 0,
                    state: PendingSseEvent::default(),
                    limits: options
                        .sse_limits
                        .unwrap_or_else(|| self.sse_limits.clone()),
                    idle_timeout,
                    received_any_bytes,
                };
                stream.enforce_buffer_limit()?;

                Ok(HttpResponse::Sse(Box::new(HttpSseResponse {
                    head,
                    stream,
                })))
            }
        }
    }

    /// Serializes `body` as JSON and deserializes the response into `TResp`.
    pub async fn send_json<TReq, TResp>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: &TReq,
        auth: Option<&AuthCredentials>,
    ) -> Result<TResp, TransportError>
    where
        TReq: Serialize + ?Sized,
        TResp: DeserializeOwned,
    {
        let payload = serde_json::to_vec(body).map_err(|_| TransportError::Serialization)?;
        self.execute_json_request(
            platform,
            method,
            url,
            HttpRequestBody::Json(payload.into()),
            auth,
            self.default_transport_options(),
        )
        .await
    }

    async fn execute_json_request<TResp>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: HttpRequestBody,
        auth: Option<&AuthCredentials>,
        transport: ResolvedTransportOptions,
    ) -> Result<TResp, TransportError>
    where
        TResp: DeserializeOwned,
    {
        let provider_headers = reqwest::header::HeaderMap::new();
        match self
            .send(HttpSendRequest {
                platform,
                auth,
                method,
                url,
                body,
                response_framing: TransportResponseFraming::Json,
                options: HttpRequestOptions::json_defaults(),
                transport,
                provider_headers,
            })
            .await?
        {
            HttpResponse::Json(response) => {
                serde_json::from_value(response.body).map_err(|_| TransportError::Serialization)
            }
            _ => unreachable!("JSON mode must return JSON response"),
        }
    }

    /// Issues a JSON `GET` request and deserializes the response.
    pub async fn get_json<TResp>(
        &self,
        platform: &PlatformConfig,
        url: &str,
        auth: Option<&AuthCredentials>,
    ) -> Result<TResp, TransportError>
    where
        TResp: DeserializeOwned,
    {
        self.execute_json_request(
            platform,
            Method::GET,
            url,
            HttpRequestBody::None,
            auth,
            self.default_transport_options(),
        )
        .await
    }

    /// Convenience wrapper for [`send_json`](Self::send_json) using `POST`.
    pub async fn post_json<TReq, TResp>(
        &self,
        platform: &PlatformConfig,
        url: &str,
        body: &TReq,
        auth: Option<&AuthCredentials>,
    ) -> Result<TResp, TransportError>
    where
        TReq: Serialize + ?Sized,
        TResp: DeserializeOwned,
    {
        self.send_json(platform, Method::POST, url, body, auth)
            .await
    }

    /// Sends JSON and returns the raw JSON response body with status metadata preserved.
    ///
    /// Unlike [`send_json`](Self::send_json), this method does not fail on non-success statuses.
    pub async fn send_json_response<TReq>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: &TReq,
        auth: Option<&AuthCredentials>,
    ) -> Result<HttpJsonResponse, TransportError>
    where
        TReq: Serialize + ?Sized,
    {
        let payload: Bytes = serde_json::to_vec(body)
            .map_err(|_| TransportError::Serialization)?
            .into();
        let request_body = HttpRequestBody::Json(payload);
        let options = HttpRequestOptions::json_defaults();
        let transport = self.default_transport_options();
        let provider_headers = reqwest::header::HeaderMap::new();

        let (response, header_config) = self
            .send_request_with_retry(RequestExecution {
                platform,
                auth,
                method,
                url,
                body: &request_body,
                response_framing: TransportResponseFraming::Json,
                options: &options,
                transport: &transport,
                provider_headers: &provider_headers,
            })
            .await?;
        let head = build_response_head(&response, &header_config);
        let body = response.json::<Value>().await?;

        Ok(HttpJsonResponse { head, body })
    }

    /// Convenience wrapper for [`send_json_response`](Self::send_json_response) using `POST`.
    pub async fn post_json_value<TReq>(
        &self,
        platform: &PlatformConfig,
        url: &str,
        body: &TReq,
        auth: Option<&AuthCredentials>,
    ) -> Result<HttpJsonResponse, TransportError>
    where
        TReq: Serialize + ?Sized,
    {
        self.send_json_response(platform, Method::POST, url, body, auth)
            .await
    }

    /// Sends a request and returns the response body as raw bytes.
    pub async fn send_bytes_request(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: HttpRequestBody,
        auth: Option<&AuthCredentials>,
        options: HttpRequestOptions,
        transport: ResolvedTransportOptions,
    ) -> Result<HttpBytesResponse, TransportError> {
        let provider_headers = reqwest::header::HeaderMap::new();
        match self
            .send(HttpSendRequest {
                platform,
                auth,
                method,
                url,
                body,
                response_framing: TransportResponseFraming::Bytes,
                options,
                transport,
                provider_headers,
            })
            .await?
        {
            HttpResponse::Bytes(response) => Ok(response),
            _ => unreachable!("bytes mode must return bytes response"),
        }
    }

    /// Sends a request and returns an SSE stream.
    ///
    /// When not provided explicitly, this method defaults the `accept` header, expected content
    /// type, and stream idle timeout to SSE-appropriate values. Retries only happen before the
    /// stream has started.
    pub async fn send_sse_request(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: HttpRequestBody,
        auth: Option<&AuthCredentials>,
        mut options: HttpRequestOptions,
        transport: ResolvedTransportOptions,
    ) -> Result<HttpSseResponse, TransportError> {
        if options.accept.is_none() {
            options.accept = Some(HeaderValue::from_static("text/event-stream"));
        }
        if options.expected_content_type.is_none() {
            options.expected_content_type = Some("text/event-stream".to_string());
        }

        let provider_headers = reqwest::header::HeaderMap::new();
        match self
            .send(HttpSendRequest {
                platform,
                auth,
                method,
                url,
                body,
                response_framing: TransportResponseFraming::Sse,
                options,
                transport,
                provider_headers,
            })
            .await?
        {
            HttpResponse::Sse(response) => Ok(*response),
            _ => unreachable!("SSE mode must return SSE response"),
        }
    }

    /// Opens an SSE stream with a `GET` request.
    pub async fn get_sse(
        &self,
        platform: &PlatformConfig,
        url: &str,
        auth: Option<&AuthCredentials>,
    ) -> Result<HttpSseResponse, TransportError> {
        self.send_sse_request(
            platform,
            Method::GET,
            url,
            HttpRequestBody::None,
            auth,
            HttpRequestOptions::default(),
            self.default_transport_options(),
        )
        .await
    }

    /// Serializes `body` as JSON and opens an SSE stream with `method`.
    pub async fn send_sse<TReq>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: &TReq,
        auth: Option<&AuthCredentials>,
    ) -> Result<HttpSseResponse, TransportError>
    where
        TReq: Serialize + ?Sized,
    {
        let payload: Bytes = serde_json::to_vec(body)
            .map_err(|_| TransportError::Serialization)?
            .into();

        self.send_sse_request(
            platform,
            method,
            url,
            HttpRequestBody::Json(payload),
            auth,
            HttpRequestOptions::sse_defaults(),
            self.default_transport_options(),
        )
        .await
    }

    /// Convenience wrapper for [`send_sse`](Self::send_sse) using `POST`.
    pub async fn post_sse<TReq>(
        &self,
        platform: &PlatformConfig,
        url: &str,
        body: &TReq,
        auth: Option<&AuthCredentials>,
    ) -> Result<HttpSseResponse, TransportError>
    where
        TReq: Serialize + ?Sized,
    {
        self.send_sse(platform, Method::POST, url, body, auth).await
    }

    async fn read_stream_chunk(
        &self,
        response: &mut reqwest::Response,
        head: &HttpResponseHead,
        idle_timeout: Option<Duration>,
        timeout_stage: TimeoutStage,
    ) -> Result<Option<Bytes>, TransportError> {
        match idle_timeout {
            Some(idle_timeout) => {
                match tokio::time::timeout(idle_timeout, response.chunk()).await {
                    Ok(Ok(chunk)) => Ok(chunk),
                    Ok(Err(error)) => Err(TransportError::StreamTerminated {
                        reason: StreamTerminationReason::Disconnect,
                        message: error.to_string(),
                        head: Box::new(head.clone()),
                    }),
                    Err(_) => Err(TransportError::Timeout {
                        stage: timeout_stage,
                    }),
                }
            }
            None => response.chunk().await.map_err(TransportError::from),
        }
    }

    pub(crate) async fn sleep_before_retry(&self, policy: &RetryPolicy, attempt: u8) {
        let retry_index = attempt.saturating_sub(1);
        let backoff = policy.backoff_duration_for_retry(retry_index);
        tokio::time::sleep(backoff).await;
    }

    pub(crate) async fn send_request_with_retry(
        &self,
        request: RequestExecution<'_>,
    ) -> Result<(reqwest::Response, HeaderConfig), TransportError> {
        let header_config = self.build_header_config(
            request.platform,
            request.auth,
            request.transport,
            request.provider_headers,
        )?;
        let max_attempts = request.transport.retry_policy.max_attempts.max(1);
        let mut attempt: u8 = 0;
        let uses_stream_timeouts =
            matches!(request.response_framing, TransportResponseFraming::Sse);

        loop {
            attempt += 1;

            let mut request_builder = self
                .client
                .request(request.method.clone(), request.url)
                .headers(header_config.headers.clone());

            if uses_stream_timeouts {
                let setup_timeout = request
                    .transport
                    .timeouts
                    .stream_setup_timeout
                    .unwrap_or(self.stream_timeout);
                request_builder = request_builder.timeout(setup_timeout);
            } else {
                let request_timeout = request
                    .transport
                    .timeouts
                    .request_timeout
                    .unwrap_or(self.request_timeout);
                request_builder = request_builder.timeout(request_timeout);
            }

            if let Some(accept) = &request.options.accept {
                request_builder = request_builder.header(reqwest::header::ACCEPT, accept.clone());
            }

            match request.body {
                HttpRequestBody::None => {}
                HttpRequestBody::Json(payload) => {
                    request_builder = request_builder
                        .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                        .body(payload.clone());
                }
                HttpRequestBody::Bytes { content_type, body } => {
                    if let Some(content_type) = content_type {
                        request_builder =
                            request_builder.header(CONTENT_TYPE, content_type.clone());
                    }
                    request_builder = request_builder.body(body.clone());
                }
            }

            let response = match request_builder.send().await {
                Ok(resp) => resp,
                Err(err) => {
                    if attempt < max_attempts && is_retryable_transport(&err) {
                        self.sleep_before_retry(&request.transport.retry_policy, attempt)
                            .await;
                        continue;
                    }

                    let stage = if uses_stream_timeouts {
                        TimeoutStage::StreamSetup
                    } else {
                        TimeoutStage::Request
                    };
                    return Err(map_reqwest_error(err, stage));
                }
            };

            let status = response.status();
            if !status.is_success()
                && attempt < max_attempts
                && request.transport.retry_policy.should_retry_status(status)
            {
                self.sleep_before_retry(&request.transport.retry_policy, attempt)
                    .await;
                continue;
            }

            return Ok((response, header_config));
        }
    }

    fn default_transport_options(&self) -> ResolvedTransportOptions {
        ResolvedTransportOptions {
            request_id_header_override: None,
            route_extra_headers: Default::default(),
            attempt_extra_headers: Default::default(),
            timeouts: TransportTimeoutOverrides {
                request_timeout: Some(self.request_timeout),
                stream_setup_timeout: Some(self.stream_timeout),
                stream_idle_timeout: Some(self.stream_timeout),
            },
            retry_policy: self.retry_policy.clone(),
        }
    }
}

fn is_retryable_transport(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

fn map_reqwest_error(error: reqwest::Error, stage: TimeoutStage) -> TransportError {
    if error.is_timeout() {
        return TransportError::Timeout { stage };
    }

    TransportError::Request(error)
}
