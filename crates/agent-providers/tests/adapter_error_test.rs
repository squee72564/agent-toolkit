use std::error::Error as StdError;

use agent_core::types::ProviderKind;
use agent_providers::error::{AdapterError, AdapterErrorKind, AdapterOperation};

#[test]
fn adapter_error_new_initializes_expected_defaults() {
    let error = AdapterError::new(
        AdapterErrorKind::Validation,
        ProviderKind::OpenAi,
        AdapterOperation::PlanRequest,
        "invalid request",
    );

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.provider, ProviderKind::OpenAi);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert_eq!(error.message, "invalid request");
    assert!(error.source_ref().is_none());
    assert_eq!(error.status_code, None);
    assert_eq!(error.request_id, None);
    assert_eq!(error.provider_code, None);
}

#[test]
fn adapter_error_with_source_exposes_source() {
    let error = AdapterError::with_source(
        AdapterErrorKind::Decode,
        ProviderKind::Anthropic,
        AdapterOperation::DecodeResponse,
        "decode failed",
        std::io::Error::other("bad json"),
    );

    let source = error.source_ref().expect("source should exist");
    assert!(source.to_string().contains("bad json"));

    let std_source = StdError::source(&error).expect("std error source should exist");
    assert!(std_source.to_string().contains("bad json"));
}

#[test]
fn adapter_error_metadata_builders_set_values() {
    let error = AdapterError::new(
        AdapterErrorKind::Upstream,
        ProviderKind::OpenRouter,
        AdapterOperation::DecodeResponse,
        "upstream failure",
    )
    .with_status_code(429)
    .with_request_id(" req_123 ")
    .with_provider_code(" rate_limit ");

    assert_eq!(error.status_code, Some(429));
    assert_eq!(error.request_id.as_deref(), Some("req_123"));
    assert_eq!(error.provider_code.as_deref(), Some("rate_limit"));
}

#[test]
fn adapter_error_metadata_builders_normalize_empty_to_none() {
    let error = AdapterError::new(
        AdapterErrorKind::Transport,
        ProviderKind::OpenAi,
        AdapterOperation::BuildHttpRequest,
        "transport failed",
    )
    .with_request_id("   ")
    .with_provider_code("");

    assert_eq!(error.request_id, None);
    assert_eq!(error.provider_code, None);
}

#[test]
fn adapter_error_builder_chain_preserves_core_fields() {
    let error = AdapterError::new(
        AdapterErrorKind::ProtocolViolation,
        ProviderKind::Anthropic,
        AdapterOperation::DecodeResponse,
        "schema mismatch",
    )
    .with_status_code(500)
    .with_request_id("req-999")
    .with_provider_code("internal_error");

    assert_eq!(error.kind, AdapterErrorKind::ProtocolViolation);
    assert_eq!(error.provider, ProviderKind::Anthropic);
    assert_eq!(error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(error.message, "schema mismatch");
    assert_eq!(error.status_code, Some(500));
    assert_eq!(error.request_id.as_deref(), Some("req-999"));
    assert_eq!(error.provider_code.as_deref(), Some("internal_error"));
}
