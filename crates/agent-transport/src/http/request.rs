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

use crate::http::transport::{HttpTransport, TransportError};

const REQUEST_ID_HEADER_KEY: &str = "transport.request_id_header";
const CUSTOM_HEADER_PREFIX: &str = "transport.header.";

#[derive(Debug, Clone)]
pub struct HttpJsonResponse {
    pub status: StatusCode,
    pub body: Value,
    pub request_id: Option<String>,
}

pub struct HeaderConfig {
    pub headers: HeaderMap,
    pub request_id_header: HeaderName,
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
    ) -> Result<(reqwest::Response, HeaderConfig), TransportError> {
        let header_config = self.build_header_config(platform, ctx)?;
        let max_attempts = self.retry_policy.max_attempts.max(1);
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
                    if attempt < max_attempts && is_retryable_transport(&err) {
                        self.sleep_before_retry(attempt).await;
                        continue;
                    }

                    return Err(TransportError::Request(err));
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
