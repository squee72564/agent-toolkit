use std::collections::BTreeMap;
use std::io;

use agent_core::types::{AdapterContext, AuthCredentials, AuthStyle};
use agent_transport::{RetryPolicy, TransportError};
use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};

use crate::support::{TestResult, default_platform, default_transport};

#[test]
fn build_header_config_applies_default_auth_and_metadata_headers() -> TestResult {
    let mut platform = default_platform(AuthStyle::Bearer);
    platform.default_headers.insert(
        HeaderName::from_static("x-default"),
        HeaderValue::from_static("base"),
    );

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.request_id_header".to_string(),
        "x-trace-id".to_string(),
    );
    metadata.insert("transport.header.x-meta".to_string(), "meta".to_string());

    let ctx = AdapterContext {
        metadata,
        auth_token: Some(AuthCredentials::Token("secret-token".to_string())),
    };

    let transport = default_transport(RetryPolicy::default());
    let config = transport.build_header_config(&platform, &ctx)?;

    assert_eq!(
        config.request_id_header,
        HeaderName::from_static("x-trace-id")
    );
    assert_eq!(
        config.headers.get("x-default"),
        Some(&HeaderValue::from_static("base"))
    );
    assert_eq!(
        config.headers.get("x-meta"),
        Some(&HeaderValue::from_static("meta"))
    );
    assert_eq!(
        config.headers.get(AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer secret-token"))
    );

    Ok(())
}

#[test]
fn build_header_config_rejects_invalid_custom_header_name() -> TestResult {
    let platform = default_platform(AuthStyle::None);
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.header.invalid header".to_string(),
        "value".to_string(),
    );

    let ctx = AdapterContext {
        metadata,
        auth_token: None,
    };

    let transport = default_transport(RetryPolicy::default());
    let error = match transport.build_header_config(&platform, &ctx) {
        Ok(_) => return Err(io::Error::other("expected invalid header name error").into()),
        Err(error) => error,
    };

    assert!(matches!(error, TransportError::InvalidHeaderName));
    Ok(())
}

#[test]
fn build_header_config_rejects_invalid_custom_header_value() -> TestResult {
    let platform = default_platform(AuthStyle::None);
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.header.x-bad".to_string(),
        "line1\nline2".to_string(),
    );

    let ctx = AdapterContext {
        metadata,
        auth_token: None,
    };

    let transport = default_transport(RetryPolicy::default());
    let error = match transport.build_header_config(&platform, &ctx) {
        Ok(_) => return Err(io::Error::other("expected invalid header value error").into()),
        Err(error) => error,
    };

    assert!(matches!(error, TransportError::InvalidHeaderValue));
    Ok(())
}
