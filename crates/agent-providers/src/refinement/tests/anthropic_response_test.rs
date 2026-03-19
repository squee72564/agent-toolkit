use crate::interfaces::adapter_for;
use crate::error::{AdapterErrorKind, AdapterOperation};
use agent_core::{ProviderKind, ResponseFormat};

#[test]
fn anthropic_response_error_maps_into_adapter_error() {
    let adapter_error = adapter_for(ProviderKind::Anthropic)
        .decode_response_json(
            serde_json::json!({"model":"claude-sonnet-4-6"}),
            &ResponseFormat::Text,
        )
        .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderKind::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert!(!adapter_error.message.is_empty());
}

#[test]
fn anthropic_upstream_error_maps_into_adapter_error() {
    let adapter_error = adapter_for(ProviderKind::Anthropic)
        .decode_response_json(
            serde_json::json!({
                "type":"error",
                "request_id":"req_test_123",
                "error":{"message":"provider said no","type":"invalid_request_error"}
            }),
            &ResponseFormat::Text,
        )
        .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderKind::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert!(adapter_error.message.contains("provider said no"));
    assert_eq!(
        adapter_error.provider_code.as_deref(),
        Some("invalid_request_error")
    );
    assert_eq!(adapter_error.request_id.as_deref(), Some("req_test_123"));
}

#[test]
fn anthropic_protocol_violation_error_maps_into_adapter_error() {
    let adapter_error = adapter_for(ProviderKind::Anthropic)
        .decode_response_json(
            serde_json::json!({"content":[],"role":"assistant","model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":"bad"}),
            &ResponseFormat::Text,
        )
        .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderKind::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert!(matches!(
        adapter_error.kind,
        AdapterErrorKind::ProtocolViolation | AdapterErrorKind::Decode
    ));
    assert!(!adapter_error.message.is_empty());
}
