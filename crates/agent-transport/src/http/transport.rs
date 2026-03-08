use std::time::Duration;

use agent_core::{AdapterContext, PlatformConfig};
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
    HttpResponse, HttpResponseHead, HttpResponseMode, HttpSendRequest, RequestExecution,
};
use crate::http::response::{build_response_head, content_type_matches};
use crate::http::retry_policy::RetryPolicy;
use crate::http::sse::{HttpSseResponse, HttpSseStream, PendingSseEvent, SseLimits};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("invalid header name")]
    InvalidHeaderName,
    #[error("invalid header value")]
    InvalidHeaderValue,
    #[error("serialization error")]
    Serialization,
    #[error("request timed out during {stage}")]
    Timeout { stage: TimeoutStage },
    #[error("unexpected HTTP status {head:?}")]
    Status {
        head: Box<crate::http::HttpResponseHead>,
    },
    #[error("unexpected response content-type: expected {expected}, got {actual:?}")]
    ContentTypeMismatch {
        expected: String,
        actual: Option<String>,
        head: Box<crate::http::HttpResponseHead>,
    },
    #[error("stream terminated unexpectedly ({reason}): {message}")]
    StreamTerminated {
        reason: StreamTerminationReason,
        message: String,
        head: Box<crate::http::HttpResponseHead>,
    },
    #[error("invalid SSE stream: {0}")]
    SseParse(String),
    #[error("{kind} exceeded limit: {size} > {max}")]
    SseLimit {
        kind: &'static str,
        size: usize,
        max: usize,
    },
    #[error("request error: {0}")]
    Request(reqwest::Error),
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutStage {
    #[error("request")]
    Request,
    #[error("stream setup")]
    StreamSetup,
    #[error("first byte")]
    FirstByte,
    #[error("stream idle")]
    StreamIdle,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum StreamTerminationReason {
    #[error("disconnect")]
    Disconnect,
    #[error("idle timeout")]
    IdleTimeout,
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

#[derive(Debug, Clone)]
pub struct HttpTransport {
    pub(crate) client: reqwest::Client,
    pub(crate) retry_policy: RetryPolicy,
    pub(crate) request_timeout: Duration,
    pub(crate) stream_timeout: Duration,
    pub(crate) sse_limits: SseLimits,
}

impl HttpTransport {
    pub fn builder(client: reqwest::Client) -> HttpTransportBuilder {
        HttpTransportBuilder {
            client,
            retry_policy: RetryPolicy::default(),
            request_timeout: Duration::from_secs(30),
            stream_timeout: Duration::from_secs(30),
            sse_limits: SseLimits::default(),
        }
    }

    pub fn build_header_config(
        &self,
        platform: &PlatformConfig,
        ctx: &AdapterContext,
    ) -> Result<HeaderConfig, TransportError> {
        build_header_config(platform, ctx)
    }

    pub async fn send(&self, request: HttpSendRequest<'_>) -> Result<HttpResponse, TransportError> {
        let HttpSendRequest {
            platform,
            method,
            url,
            body,
            ctx,
            options,
            response_mode,
        } = request;
        let (mut response, header_config) = self
            .send_request_with_retry(RequestExecution {
                platform,
                method,
                url,
                body: &body,
                ctx,
                options: &options,
                response_mode,
            })
            .await?;

        let head = build_response_head(&response, &header_config);
        if !(head.status.is_success()
            || matches!(response_mode, HttpResponseMode::Json) && options.allow_error_status)
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

        match response_mode {
            HttpResponseMode::Json => {
                let body = response.json::<Value>().await?;
                Ok(HttpResponse::Json(HttpJsonResponse { head, body }))
            }
            HttpResponseMode::Bytes => {
                let body = response.bytes().await?;
                Ok(HttpResponse::Bytes(HttpBytesResponse { head, body }))
            }
            HttpResponseMode::Sse => {
                let mut buffer = BytesMut::new();
                let idle_timeout = options.stream_idle_timeout;
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

    pub async fn send_json<TReq, TResp>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: &TReq,
        ctx: &AdapterContext,
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
            ctx,
        )
        .await
    }

    async fn execute_json_request<TResp>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: HttpRequestBody,
        ctx: &AdapterContext,
    ) -> Result<TResp, TransportError>
    where
        TResp: DeserializeOwned,
    {
        match self
            .send(HttpSendRequest {
                platform,
                method,
                url,
                body,
                ctx,
                options: HttpRequestOptions::json_defaults(),
                response_mode: HttpResponseMode::Json,
            })
            .await?
        {
            HttpResponse::Json(response) => {
                serde_json::from_value(response.body).map_err(|_| TransportError::Serialization)
            }
            _ => unreachable!("JSON mode must return JSON response"),
        }
    }

    pub async fn get_json<TResp>(
        &self,
        platform: &PlatformConfig,
        url: &str,
        ctx: &AdapterContext,
    ) -> Result<TResp, TransportError>
    where
        TResp: DeserializeOwned,
    {
        self.execute_json_request(platform, Method::GET, url, HttpRequestBody::None, ctx)
            .await
    }

    pub async fn post_json<TReq, TResp>(
        &self,
        platform: &PlatformConfig,
        url: &str,
        body: &TReq,
        ctx: &AdapterContext,
    ) -> Result<TResp, TransportError>
    where
        TReq: Serialize + ?Sized,
        TResp: DeserializeOwned,
    {
        self.send_json(platform, Method::POST, url, body, ctx).await
    }

    pub async fn send_json_response<TReq>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: &TReq,
        ctx: &AdapterContext,
    ) -> Result<HttpJsonResponse, TransportError>
    where
        TReq: Serialize + ?Sized,
    {
        let payload: Bytes = serde_json::to_vec(body)
            .map_err(|_| TransportError::Serialization)?
            .into();
        let request_body = HttpRequestBody::Json(payload);
        let options = HttpRequestOptions::json_defaults();

        let (response, header_config) = self
            .send_request_with_retry(RequestExecution {
                platform,
                method,
                url,
                body: &request_body,
                ctx,
                options: &options,
                response_mode: HttpResponseMode::Json,
            })
            .await?;
        let head = build_response_head(&response, &header_config);
        let body = response.json::<Value>().await?;

        Ok(HttpJsonResponse { head, body })
    }

    pub async fn post_json_value<TReq>(
        &self,
        platform: &PlatformConfig,
        url: &str,
        body: &TReq,
        ctx: &AdapterContext,
    ) -> Result<HttpJsonResponse, TransportError>
    where
        TReq: Serialize + ?Sized,
    {
        self.send_json_response(platform, Method::POST, url, body, ctx)
            .await
    }

    pub async fn send_bytes_request(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: HttpRequestBody,
        ctx: &AdapterContext,
        options: HttpRequestOptions,
    ) -> Result<HttpBytesResponse, TransportError> {
        match self
            .send(HttpSendRequest {
                platform,
                method,
                url,
                body,
                ctx,
                options,
                response_mode: HttpResponseMode::Bytes,
            })
            .await?
        {
            HttpResponse::Bytes(response) => Ok(response),
            _ => unreachable!("bytes mode must return bytes response"),
        }
    }

    pub async fn send_sse_request(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: HttpRequestBody,
        ctx: &AdapterContext,
        mut options: HttpRequestOptions,
    ) -> Result<HttpSseResponse, TransportError> {
        if options.accept.is_none() {
            options.accept = Some(HeaderValue::from_static("text/event-stream"));
        }
        if options.expected_content_type.is_none() {
            options.expected_content_type = Some("text/event-stream".to_string());
        }
        if options.stream_idle_timeout.is_none() {
            options.stream_idle_timeout = Some(self.stream_timeout);
        }

        match self
            .send(HttpSendRequest {
                platform,
                method,
                url,
                body,
                ctx,
                options,
                response_mode: HttpResponseMode::Sse,
            })
            .await?
        {
            HttpResponse::Sse(response) => Ok(*response),
            _ => unreachable!("SSE mode must return SSE response"),
        }
    }

    pub async fn get_sse(
        &self,
        platform: &PlatformConfig,
        url: &str,
        ctx: &AdapterContext,
    ) -> Result<HttpSseResponse, TransportError> {
        self.send_sse_request(
            platform,
            Method::GET,
            url,
            HttpRequestBody::None,
            ctx,
            HttpRequestOptions::default(),
        )
        .await
    }

    pub async fn send_sse<TReq>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: &TReq,
        ctx: &AdapterContext,
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
            ctx,
            HttpRequestOptions::sse_defaults(),
        )
        .await
    }

    pub async fn post_sse<TReq>(
        &self,
        platform: &PlatformConfig,
        url: &str,
        body: &TReq,
        ctx: &AdapterContext,
    ) -> Result<HttpSseResponse, TransportError>
    where
        TReq: Serialize + ?Sized,
    {
        self.send_sse(platform, Method::POST, url, body, ctx).await
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

    pub(crate) async fn sleep_before_retry(&self, attempt: u8) {
        let retry_index = attempt.saturating_sub(1);
        let backoff = self.retry_policy.backoff_duration_for_retry(retry_index);
        tokio::time::sleep(backoff).await;
    }

    pub(crate) async fn send_request_with_retry(
        &self,
        request: RequestExecution<'_>,
    ) -> Result<(reqwest::Response, HeaderConfig), TransportError> {
        let header_config = self.build_header_config(request.platform, request.ctx)?;
        let max_attempts = self.retry_policy.max_attempts.max(1);
        let mut attempt: u8 = 0;
        let uses_stream_timeouts = matches!(request.response_mode, HttpResponseMode::Sse);

        loop {
            attempt += 1;

            let mut request_builder = self
                .client
                .request(request.method.clone(), request.url)
                .headers(header_config.headers.clone());

            if uses_stream_timeouts {
                let setup_timeout = request
                    .options
                    .stream_setup_timeout
                    .unwrap_or(self.stream_timeout);
                request_builder = request_builder.timeout(setup_timeout);
            } else {
                let request_timeout = request
                    .options
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
                        self.sleep_before_retry(attempt).await;
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
                && self.retry_policy.should_retry_status(status)
            {
                self.sleep_before_retry(attempt).await;
                continue;
            }

            return Ok((response, header_config));
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
