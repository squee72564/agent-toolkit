use std::time::Duration;

use agent_core::{AdapterContext, PlatformConfig};
use bytes::{Bytes, BytesMut};
use reqwest::{Method, header::InvalidHeaderName, header::InvalidHeaderValue};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

use crate::http::builder::HttpTransportBuilder;
use crate::http::request::{
    HttpBytesResponse, HttpJsonResponse, HttpRequestBody, HttpRequestOptions, HttpResponse,
    HttpResponseMode, HttpSendRequest, RequestExecution, build_response_head, content_type_matches,
};
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
    pub client: reqwest::Client,
    pub retry_policy: RetryPolicy,
    pub request_timeout: Duration,
    pub stream_timeout: Duration,
    pub sse_limits: SseLimits,
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
        if !head.status.is_success() {
            return Err(TransportError::Status {
                head: Box::new(head),
            });
        }

        if let Some(expected) = options.expected_content_type.as_deref()
            && !content_type_matches(&head.headers, expected)
        {
            let actual = head
                .headers
                .get(reqwest::header::CONTENT_TYPE)
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
                    .read_stream_chunk(&mut response, idle_timeout, TimeoutStage::FirstByte)
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
        let payload = serde_json::to_vec(body).map_err(|_| TransportError::Serialization)?;
        self.execute_json_request(
            platform,
            Method::POST,
            url,
            HttpRequestBody::Json(payload.into()),
            ctx,
        )
        .await
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
        let payload: Bytes = serde_json::to_vec(body)
            .map_err(|_| TransportError::Serialization)?
            .into();
        let request_body = HttpRequestBody::Json(payload);
        let options = HttpRequestOptions::json_defaults();

        let (response, header_config) = self
            .send_request_with_retry(RequestExecution {
                platform,
                method: Method::POST,
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

        match self
            .send(HttpSendRequest {
                platform,
                method,
                url,
                body: HttpRequestBody::Json(payload),
                ctx,
                options: HttpRequestOptions::sse_defaults(),
                response_mode: HttpResponseMode::Sse,
            })
            .await?
        {
            HttpResponse::Sse(response) => Ok(*response),
            _ => unreachable!("SSE mode must return SSE response"),
        }
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
                        head: Box::new(crate::http::HttpResponseHead {
                            status: response.status(),
                            headers: response.headers().clone(),
                            request_id: None,
                        }),
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
}
