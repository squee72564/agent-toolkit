use std::time::Duration;

use agent_core::{AdapterContext, AuthCredentials, AuthStyle, PlatformConfig};
use base64::Engine;
use bytes::Bytes;
use reqwest::{
    Method, StatusCode,
    header::{
        AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, InvalidHeaderName,
        InvalidHeaderValue,
    },
};
use serde_json::Value;

use crate::http::sse::SseLimits;
use crate::http::transport::{HttpTransport, TimeoutStage, TransportError};

const REQUEST_ID_HEADER_KEY: &str = "transport.request_id_header";
const CUSTOM_HEADER_PREFIX: &str = "transport.header.";

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

    pub(crate) fn json_defaults() -> Self {
        Self::default()
    }

    pub(crate) fn sse_defaults() -> Self {
        Self::default()
            .with_accept(HeaderValue::from_static("text/event-stream"))
            .with_expected_content_type("text/event-stream")
    }
}

impl HttpTransport {
    pub fn build_header_config(
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

pub(crate) fn extract_request_id(
    headers: &HeaderMap,
    header_config: &HeaderConfig,
) -> Option<String> {
    headers
        .get(&header_config.request_id_header)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn parse_header_name(raw: &str) -> Result<HeaderName, InvalidHeaderName> {
    HeaderName::from_bytes(raw.as_bytes())
}

fn is_retryable_transport(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

pub(crate) fn build_response_head(
    response: &reqwest::Response,
    header_config: &HeaderConfig,
) -> HttpResponseHead {
    HttpResponseHead {
        status: response.status(),
        headers: response.headers().clone(),
        request_id: extract_request_id(response.headers(), header_config),
    }
}

pub(crate) fn content_type_matches(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.split(';').next())
        .map(str::trim)
        .is_some_and(|content_type| content_type.eq_ignore_ascii_case(expected))
}

fn map_reqwest_error(error: reqwest::Error, stage: TimeoutStage) -> TransportError {
    if error.is_timeout() {
        return TransportError::Timeout { stage };
    }

    TransportError::Request(error)
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
            let encoded = base64::engine::general_purpose::STANDARD.encode(token);
            let value = format!("Basic {encoded}");
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&value)?);
            Ok(())
        }
    }
}
