use std::time::Duration;

use agent_core::{AdapterContext, PlatformConfig};
use bytes::Bytes;
use reqwest::{Method, header::InvalidHeaderName, header::InvalidHeaderValue};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

use crate::http::builder::HttpTransportBuilder;
use crate::http::request::{HttpJsonResponse, extract_request_id};
use crate::http::retry_policy::RetryPolicy;
use crate::http::sse::{HttpSseResponse, HttpSseStream, PendingSseEvent};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("invalid header name")]
    InvalidHeaderName,
    #[error("invalid header value")]
    InvalidHeaderValue,
    #[error("request error: {0}")]
    Request(reqwest::Error),
    #[error("serialization error")]
    Serialization,
    #[error("invalid SSE stream: {0}")]
    SseParse(String),
}

impl From<reqwest::Error> for TransportError {
    fn from(err: reqwest::Error) -> Self {
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
    pub timeout: Duration,
}

impl HttpTransport {
    pub fn builder(client: reqwest::Client) -> HttpTransportBuilder {
        HttpTransportBuilder {
            client,
            retry_policy: RetryPolicy::default(),
            timeout: Duration::from_secs(30),
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
        let (response, _) = self
            .send_request_with_retry(platform, method, url, body, ctx)
            .await?;

        if let Err(error) = response.error_for_status_ref() {
            return Err(TransportError::Request(error));
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
            .send_request_with_retry(platform, Method::POST, url, Some(payload), ctx)
            .await?;
        let status = response.status();
        let request_id = extract_request_id(response.headers(), &header_config);
        let body = response.json::<Value>().await?;

        Ok(HttpJsonResponse {
            status,
            body,
            request_id,
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
        let payload: Bytes = serde_json::to_vec(body)
            .map_err(|_| TransportError::Serialization)?
            .into();
        let (response, header_config) = self
            .send_request_with_retry(platform, Method::POST, url, Some(payload), ctx)
            .await?;

        if let Err(error) = response.error_for_status_ref() {
            return Err(TransportError::Request(error));
        }

        let status = response.status();
        let headers = response.headers().clone();
        let request_id = extract_request_id(&headers, &header_config);

        Ok(HttpSseResponse {
            status,
            headers,
            request_id,
            stream: HttpSseStream {
                response,
                buffer: Vec::new(),
                state: PendingSseEvent::default(),
            },
        })
    }

    pub(crate) async fn sleep_before_retry(&self, attempt: u8) {
        let retry_index = attempt.saturating_sub(1);
        let backoff = self.retry_policy.backoff_duration_for_retry(retry_index);
        tokio::time::sleep(backoff).await;
    }
}
