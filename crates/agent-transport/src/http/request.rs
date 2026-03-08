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

pub struct HeaderConfig {
    pub headers: HeaderMap,
    pub request_id_header: HeaderName,
}

#[derive(Debug, Clone)]
pub(crate) struct HttpRequestOptions {
    pub accept: Option<HeaderValue>,
    pub body_content_type: Option<HeaderValue>,
    pub timeout_mode: TimeoutMode,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TimeoutMode {
    Request,
    StreamSetup,
}

impl HttpRequestOptions {
    pub(crate) fn json() -> Self {
        Self {
            accept: None,
            body_content_type: Some(HeaderValue::from_static("application/json")),
            timeout_mode: TimeoutMode::Request,
        }
    }

    pub(crate) fn sse() -> Self {
        Self {
            accept: Some(HeaderValue::from_static("text/event-stream")),
            body_content_type: Some(HeaderValue::from_static("application/json")),
            timeout_mode: TimeoutMode::StreamSetup,
        }
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
        platform: &PlatformConfig,
        method: Method,
        url: &str,
        body: Option<Bytes>,
        ctx: &AdapterContext,
        options: &HttpRequestOptions,
    ) -> Result<(reqwest::Response, HeaderConfig), TransportError> {
        let header_config = self.build_header_config(platform, ctx)?;
        let max_attempts = self.retry_policy.max_attempts.max(1);
        let mut attempt: u8 = 0;

        loop {
            attempt += 1;

            let mut request_builder = self
                .client
                .request(method.clone(), url)
                .headers(header_config.headers.clone());

            if matches!(options.timeout_mode, TimeoutMode::Request) {
                request_builder = request_builder.timeout(self.request_timeout);
            }

            if let Some(accept) = &options.accept {
                request_builder = request_builder.header(reqwest::header::ACCEPT, accept.clone());
            }

            if let Some(payload) = &body {
                request_builder = request_builder
                    .header(
                        CONTENT_TYPE,
                        options.body_content_type.clone().unwrap_or_else(|| {
                            HeaderValue::from_static("application/octet-stream")
                        }),
                    )
                    .body(payload.clone());
            }

            let response = match options.timeout_mode {
                TimeoutMode::Request => match request_builder.send().await {
                    Ok(resp) => resp,
                    Err(err) => {
                        if attempt < max_attempts && is_retryable_transport(&err) {
                            self.sleep_before_retry(attempt).await;
                            continue;
                        }

                        return Err(map_reqwest_error(err, TimeoutMode::Request));
                    }
                },
                TimeoutMode::StreamSetup => {
                    let setup_timeout = self.stream_timeout;
                    match tokio::time::timeout(setup_timeout, request_builder.send()).await {
                        Ok(Ok(resp)) => resp,
                        Ok(Err(err)) => {
                            if attempt < max_attempts && is_retryable_transport(&err) {
                                self.sleep_before_retry(attempt).await;
                                continue;
                            }

                            return Err(map_reqwest_error(err, TimeoutMode::StreamSetup));
                        }
                        Err(_) => {
                            if attempt < max_attempts {
                                self.sleep_before_retry(attempt).await;
                                continue;
                            }

                            return Err(TransportError::Timeout {
                                stage: TimeoutStage::StreamSetup,
                            });
                        }
                    }
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

fn map_reqwest_error(error: reqwest::Error, timeout_mode: TimeoutMode) -> TransportError {
    if error.is_timeout() {
        return TransportError::Timeout {
            stage: match timeout_mode {
                TimeoutMode::Request => TimeoutStage::Request,
                TimeoutMode::StreamSetup => TimeoutStage::StreamSetup,
            },
        };
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
