use agent_core::{AdapterContext, AuthCredentials, AuthStyle, PlatformConfig};
use base64::Engine;
use reqwest::header::{
    AUTHORIZATION, HeaderMap, HeaderValue, InvalidHeaderName, InvalidHeaderValue,
};

use crate::http::request::HeaderConfig;
use crate::http::transport::TransportError;

const REQUEST_ID_HEADER_KEY: &str = "transport.request_id_header";
const CUSTOM_HEADER_PREFIX: &str = "transport.header.";

pub(crate) fn build_header_config(
    platform: &PlatformConfig,
    ctx: &AdapterContext,
) -> Result<HeaderConfig, TransportError> {
    let request_id_header = ctx
        .metadata
        .get(REQUEST_ID_HEADER_KEY)
        .map(|value| parse_header_name(value))
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

fn parse_header_name(raw: &str) -> Result<reqwest::header::HeaderName, InvalidHeaderName> {
    reqwest::header::HeaderName::from_bytes(raw.as_bytes())
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
