use std::io;

use agent_core::types::{AuthCredentials, AuthStyle};
use agent_transport::{RetryPolicy, TransportError};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};

use crate::support::{TestResult, default_platform, default_resolved_transport, default_transport};

#[test]
fn build_header_config_applies_typed_header_layers_and_auth() -> TestResult {
    let mut platform = default_platform(AuthStyle::Bearer);
    platform.default_headers.insert(
        HeaderName::from_static("x-default"),
        HeaderValue::from_static("base"),
    );

    let mut transport_options = default_resolved_transport(RetryPolicy::default());
    transport_options.request_id_header_override = Some("x-trace-id".to_string());
    transport_options
        .route_extra_headers
        .insert("x-meta".to_string(), "meta".to_string());

    let transport = default_transport(RetryPolicy::default());
    let config = transport.build_header_config(
        &platform,
        Some(&AuthCredentials::Token("secret-token".to_string())),
        &transport_options,
        &HeaderMap::new(),
    )?;

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
fn build_header_config_applies_locked_header_precedence() -> TestResult {
    let mut platform = default_platform(AuthStyle::ApiKeyHeader(HeaderName::from_static(
        "x-api-key",
    )));
    platform.default_headers.insert(
        HeaderName::from_static("x-shared"),
        HeaderValue::from_static("platform"),
    );
    platform.default_headers.insert(
        HeaderName::from_static("x-platform-only"),
        HeaderValue::from_static("platform-only"),
    );

    let mut transport_options = default_resolved_transport(RetryPolicy::default());
    transport_options
        .route_extra_headers
        .insert("x-shared".to_string(), "route".to_string());
    transport_options
        .route_extra_headers
        .insert("x-route-only".to_string(), "route-only".to_string());
    transport_options
        .attempt_extra_headers
        .insert("x-shared".to_string(), "attempt".to_string());
    transport_options
        .attempt_extra_headers
        .insert("x-attempt-only".to_string(), "attempt-only".to_string());

    let mut provider_headers = HeaderMap::new();
    provider_headers.insert("x-shared", HeaderValue::from_static("provider"));
    provider_headers.insert("x-provider-only", HeaderValue::from_static("provider-only"));
    provider_headers.insert("x-api-key", HeaderValue::from_static("provider-key"));

    let transport = default_transport(RetryPolicy::default());
    let config = transport.build_header_config(
        &platform,
        Some(&AuthCredentials::Token("runtime-key".to_string())),
        &transport_options,
        &provider_headers,
    )?;

    assert_eq!(
        config.headers.get("x-shared"),
        Some(&HeaderValue::from_static("provider"))
    );
    assert_eq!(
        config.headers.get("x-platform-only"),
        Some(&HeaderValue::from_static("platform-only"))
    );
    assert_eq!(
        config.headers.get("x-route-only"),
        Some(&HeaderValue::from_static("route-only"))
    );
    assert_eq!(
        config.headers.get("x-attempt-only"),
        Some(&HeaderValue::from_static("attempt-only"))
    );
    assert_eq!(
        config.headers.get("x-provider-only"),
        Some(&HeaderValue::from_static("provider-only"))
    );
    assert_eq!(
        config.headers.get("x-api-key"),
        Some(&HeaderValue::from_static("runtime-key"))
    );

    Ok(())
}

#[test]
fn build_header_config_uses_platform_request_id_header_when_override_absent() -> TestResult {
    let mut platform = default_platform(AuthStyle::None);
    platform.request_id_header = HeaderName::from_static("x-platform-request-id");

    let transport = default_transport(RetryPolicy::default());
    let config = transport.build_header_config(
        &platform,
        None,
        &default_resolved_transport(RetryPolicy::default()),
        &HeaderMap::new(),
    )?;

    assert_eq!(
        config.request_id_header,
        HeaderName::from_static("x-platform-request-id")
    );

    Ok(())
}

#[test]
fn build_header_config_rejects_invalid_custom_header_name() -> TestResult {
    let platform = default_platform(AuthStyle::None);
    let mut transport_options = default_resolved_transport(RetryPolicy::default());
    transport_options
        .route_extra_headers
        .insert("invalid header".to_string(), "value".to_string());

    let transport = default_transport(RetryPolicy::default());
    let error =
        match transport.build_header_config(&platform, None, &transport_options, &HeaderMap::new())
        {
            Ok(_) => return Err(io::Error::other("expected invalid header name error").into()),
            Err(error) => error,
        };

    assert!(matches!(error, TransportError::InvalidHeaderName));
    Ok(())
}

#[test]
fn build_header_config_rejects_invalid_custom_header_value() -> TestResult {
    let platform = default_platform(AuthStyle::None);
    let mut transport_options = default_resolved_transport(RetryPolicy::default());
    transport_options
        .route_extra_headers
        .insert("x-bad".to_string(), "line1\nline2".to_string());

    let transport = default_transport(RetryPolicy::default());
    let error =
        match transport.build_header_config(&platform, None, &transport_options, &HeaderMap::new())
        {
            Ok(_) => return Err(io::Error::other("expected invalid header value error").into()),
            Err(error) => error,
        };

    assert!(matches!(error, TransportError::InvalidHeaderValue));
    Ok(())
}
