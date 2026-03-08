use std::time::Duration;

use agent_core::{AdapterContext, PlatformConfig};
use bytes::Bytes;
use reqwest::{Method, header::InvalidHeaderName, header::InvalidHeaderValue};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

use crate::http::builder::HttpTransportBuilder;
use crate::http::request::{
    HttpJsonResponse, HttpRequestOptions, build_response_head, content_type_matches,
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
        expected: &'static str,
        actual: Option<String>,
        head: Box<crate::http::HttpResponseHead>,
    },
    #[error("stream terminated unexpectedly: {message}")]
    StreamTerminated {
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

    async fn execute_json_request<TResp>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: Option<Bytes>,
        ctx: &AdapterContext,
    ) -> Result<TResp, TransportError>
    where
        TResp: DeserializeOwned,
    {
        let (response, header_config) = self
            .send_request_with_retry(
                platform,
                method,
                url,
                body,
                ctx,
                &HttpRequestOptions::json(),
            )
            .await?;

        if !response.status().is_success() {
            return Err(TransportError::Status {
                head: Box::new(build_response_head(&response, &header_config)),
            });
        }

        response.json::<TResp>().await.map_err(TransportError::from)
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
        self.execute_json_request(platform, Method::GET, url, None, ctx)
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
        self.execute_json_request(platform, Method::POST, url, Some(payload.into()), ctx)
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
        let (response, header_config) = self
            .send_request_with_retry(
                platform,
                Method::POST,
                url,
                Some(payload),
                ctx,
                &HttpRequestOptions::json(),
            )
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
        let (response, header_config) = self
            .send_request_with_retry(
                platform,
                method,
                url,
                Some(payload),
                ctx,
                &HttpRequestOptions::sse(),
            )
            .await?;

        let head = build_response_head(&response, &header_config);

        if !head.status.is_success() {
            return Err(TransportError::Status {
                head: Box::new(head),
            });
        }

        if !content_type_matches(&head.headers, "text/event-stream") {
            let actual = head
                .headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);

            return Err(TransportError::ContentTypeMismatch {
                expected: "text/event-stream",
                actual,
                head: Box::new(head),
            });
        }

        Ok(HttpSseResponse {
            head: head.clone(),
            stream: HttpSseStream {
                head,
                response,
                buffer: bytes::BytesMut::new(),
                buffer_offset: 0,
                state: PendingSseEvent::default(),
                limits: self.sse_limits.clone(),
            },
        })
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

    pub(crate) async fn sleep_before_retry(&self, attempt: u8) {
        let retry_index = attempt.saturating_sub(1);
        let backoff = self.retry_policy.backoff_duration_for_retry(retry_index);
        tokio::time::sleep(backoff).await;
    }
}
