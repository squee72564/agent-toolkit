use std::time::Duration;

use base64::Engine;
use reqwest::header::{
    AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, InvalidHeaderName,
    InvalidHeaderValue,
};
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;
//use serde::Serialize;
//use serde::de::DeserializeOwned;

use agent_core::types::{AdapterContext, AuthCredentials, AuthStyle, PlatformConfig};
const REQUEST_ID_HEADER_KEY: &str = "transport.request_id_header";
const CUSTOM_HEADER_PREFIX: &str = "transport.header.";

struct HeaderConfig {
    headers: HeaderMap,            // outbound
    request_id_header: HeaderName, // inbound extraction rule
}

#[derive(Debug, Clone)]
pub struct HttpJsonResponse {
    pub status: StatusCode,
    pub body: Value,
    pub request_id: Option<String>,
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub retryable_status_codes: Vec<StatusCode>,
}

impl RetryPolicy {
    fn should_retry_status(&self, status_code: StatusCode) -> bool {
        self.retryable_status_codes.contains(&status_code)
    }

    fn backoff_duration_for_retry(&self, retry_index: u8) -> Duration {
        // Exponential backoff: initial_backoff * (2 ^ retry_index), capped.
        let shift = u32::from(retry_index.min(31));
        let multiplier = 1_u32 << shift;
        self.initial_backoff
            .saturating_mul(multiplier)
            .min(self.max_backoff)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(2_000),
            retryable_status_codes: vec![
                StatusCode::REQUEST_TIMEOUT,
                StatusCode::TOO_MANY_REQUESTS,
                StatusCode::INTERNAL_SERVER_ERROR,
                StatusCode::BAD_GATEWAY,
                StatusCode::SERVICE_UNAVAILABLE,
                StatusCode::GATEWAY_TIMEOUT,
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpTransport {
    client: reqwest::Client,
    retry_policy: RetryPolicy,
    timeout: Duration,
}

#[derive(Clone)]
pub struct HttpTransportBuilder {
    client: reqwest::Client,
    retry_policy: RetryPolicy,
    timeout: Duration,
}

impl HttpTransport {
    pub fn builder(client: reqwest::Client) -> HttpTransportBuilder {
        HttpTransportBuilder {
            client,
            retry_policy: RetryPolicy::default(),
            timeout: Duration::from_secs(30),
        }
    }

    fn build_header_config(
        &self,
        platform: &PlatformConfig,
        ctx: &AdapterContext,
    ) -> Result<HeaderConfig, TransportError> {
        let request_id_header = ctx
            .metadata
            .get(REQUEST_ID_HEADER_KEY)
            .map(|v| parse_header_name(v))
            .transpose()
            .map_err(TransportError::from)?
            .unwrap_or_else(|| platform.request_id_header.clone());

        let mut headers = platform.default_headers.clone();

        if let Some(credentials) = &ctx.auth_token {
            apply_auth(&mut headers, &platform.auth_style, credentials)?;
        }

        // Cusotm metadata headers
        for (key, value) in &ctx.metadata {
            if let Some(raw_name) = key.strip_prefix(CUSTOM_HEADER_PREFIX) {
                let header_name = parse_header_name(raw_name)?;
                let header_value = HeaderValue::from_str(value)?;
                headers.insert(header_name, header_value);
            }
        }

        Ok(HeaderConfig {
            headers,
            request_id_header,
        })
    }

    async fn execute_json_request<TResp>(
        &self,
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: Option<Vec<u8>>,
        ctx: &AdapterContext,
    ) -> Result<TResp, TransportError>
    where
        TResp: DeserializeOwned,
    {
        let header_config = self.build_header_config(platform, ctx)?;
        let mut attempt: u8 = 0;

        loop {
            attempt += 1;

            let mut request_builder = self
                .client
                .request(method.clone(), url)
                .timeout(self.timeout)
                .headers(header_config.headers.clone());

            if let Some(payload) = &body {
                request_builder = request_builder
                    .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                    .body(payload.clone());
            }

            let response = match request_builder.send().await {
                Ok(resp) => resp,
                Err(err) => {
                    if attempt < self.retry_policy.max_attempts && is_retryable_transport(&err) {
                        self.sleep_before_retry(attempt).await;
                        continue;
                    }

                    return Err(TransportError::Request(err));
                }
            };

            let status = response.status();
            if status.is_success() {
                let parsed = response.json::<TResp>().await?;
                return Ok(parsed);
            }

            if attempt < self.retry_policy.max_attempts
                && self.retry_policy.should_retry_status(status)
            {
                self.sleep_before_retry(attempt).await;
                continue;
            }

            let error = response.error_for_status().unwrap_err();
            return Err(TransportError::Request(error));
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
        self.execute_json_request(platform, Method::POST, url, Some(payload), ctx)
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
        let payload = serde_json::to_vec(body).map_err(|_| TransportError::Serialization)?;
        let header_config = self.build_header_config(platform, ctx)?;
        let mut attempt: u8 = 0;

        loop {
            attempt += 1;

            let request_builder = self
                .client
                .request(Method::POST, url)
                .timeout(self.timeout)
                .headers(header_config.headers.clone())
                .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                .body(payload.clone());

            let response = match request_builder.send().await {
                Ok(resp) => resp,
                Err(err) => {
                    if attempt < self.retry_policy.max_attempts && is_retryable_transport(&err) {
                        self.sleep_before_retry(attempt).await;
                        continue;
                    }

                    return Err(TransportError::Request(err));
                }
            };

            let status = response.status();
            if !status.is_success()
                && attempt < self.retry_policy.max_attempts
                && self.retry_policy.should_retry_status(status)
            {
                self.sleep_before_retry(attempt).await;
                continue;
            }

            let request_id = response
                .headers()
                .get(&header_config.request_id_header)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let body = response.json::<Value>().await?;

            return Ok(HttpJsonResponse {
                status,
                body,
                request_id,
            });
        }
    }

    async fn sleep_before_retry(&self, attempt: u8) {
        let retry_index = attempt.saturating_sub(1);
        let backoff = self.retry_policy.backoff_duration_for_retry(retry_index);
        tokio::time::sleep(backoff).await;
    }
}

fn is_retryable_transport(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

fn apply_auth(
    headers: &mut HeaderMap,
    style: &AuthStyle,
    creds: &AuthCredentials,
) -> Result<(), InvalidHeaderValue> {
    match (style, creds) {
        (AuthStyle::None, _) => Ok(()),
        (AuthStyle::Bearer, AuthCredentials::Token(token)) => {
            let value = format!("Bearer {token}");
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&value)?);
            Ok(())
        }
        (AuthStyle::ApiKeyHeader(header_name), AuthCredentials::Token(token)) => {
            headers.insert(header_name.clone(), HeaderValue::from_str(token)?);
            Ok(())
        }
        (AuthStyle::Basic, AuthCredentials::Token(token)) => {
            // Here token is assumed to be username:password
            let encoded = base64::engine::general_purpose::STANDARD.encode(token);
            let value = format!("Basic {encoded}");
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&value)?);
            Ok(())
        }
    }
}

fn parse_header_name(raw: &str) -> Result<HeaderName, InvalidHeaderName> {
    HeaderName::from_bytes(raw.as_bytes())
}

impl HttpTransportBuilder {
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> HttpTransport {
        HttpTransport {
            client: self.client,
            retry_policy: self.retry_policy,
            timeout: self.timeout,
        }
    }
}
