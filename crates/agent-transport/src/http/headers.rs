use agent_core::{AuthCredentials, AuthStyle, PlatformConfig, ResolvedTransportOptions};
use base64::Engine;
use reqwest::header::{
    AUTHORIZATION, HeaderMap, HeaderValue, InvalidHeaderName, InvalidHeaderValue,
};

use crate::http::request::HeaderConfig;
use crate::http::transport::TransportError;

pub(crate) fn build_header_config(
    platform: &PlatformConfig,
    auth: Option<&AuthCredentials>,
    transport: &ResolvedTransportOptions,
    provider_headers: &HeaderMap,
) -> Result<HeaderConfig, TransportError> {
    let request_id_header = transport
        .request_id_header_override
        .as_deref()
        .map(parse_header_name)
        .transpose()
        .map_err(TransportError::from)?
        .unwrap_or_else(|| platform.request_id_header.clone());

    let mut headers = platform.default_headers.clone();

    for (key, value) in &transport.route_extra_headers {
        headers.insert(parse_header_name(key)?, HeaderValue::from_str(value)?);
    }

    for (key, value) in &transport.attempt_extra_headers {
        headers.insert(parse_header_name(key)?, HeaderValue::from_str(value)?);
    }

    for (key, value) in provider_headers {
        headers.insert(key.clone(), value.clone());
    }

    if let Some(credentials) = auth {
        apply_auth(&mut headers, &platform.auth_style, credentials)?;
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
