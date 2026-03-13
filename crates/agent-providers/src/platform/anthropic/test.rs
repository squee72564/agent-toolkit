use crate::error::{AdapterErrorKind, AdapterOperation};
use agent_core::{
    ContentPart, Message, MessageRole, ProviderId, Request, ResponseFormat, ToolChoice,
};

use super::{request, response};

fn base_request() -> Request {
    Request {
        model_id: "claude-sonnet-4-6".to_string(),
        stream: false,
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::text("hello")],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: std::collections::BTreeMap::new(),
    }
}

#[test]
fn anthropic_request_error_maps_into_adapter_error() {
    let adapter_error = request::plan_request(
        agent_core::Request {
            model_id: String::new(),
            ..base_request()
        },
        None,
    )
    .expect_err("planning should fail");

    assert_eq!(adapter_error.provider, ProviderId::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::PlanRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert!(!adapter_error.message.is_empty());
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn anthropic_response_error_maps_into_adapter_error() {
    let adapter_error = response::decode_response_json(
        serde_json::json!({"model":"claude-sonnet-4-6"}),
        &agent_core::ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderId::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert!(!adapter_error.message.is_empty());
}

#[test]
fn anthropic_request_error_preserves_source_chain() {
    let adapter_error = request::plan_request(
        agent_core::Request {
            model_id: String::new(),
            ..base_request()
        },
        None,
    )
    .expect_err("planning should fail");

    let spec_source = adapter_error
        .source_ref()
        .expect("adapter error should preserve spec source");
    assert!(
        spec_source.to_string().contains("validation error"),
        "expected spec error context, got: {spec_source}"
    );
}

#[test]
fn anthropic_upstream_error_maps_into_adapter_error() {
    let adapter_error = response::decode_response_json(
        serde_json::json!({"type":"error","error":{"message":"provider said no"}}),
        &agent_core::ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderId::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert!(adapter_error.message.contains("provider said no"));
}

#[test]
fn anthropic_protocol_violation_error_maps_into_adapter_error() {
    let adapter_error = response::decode_response_json(
        serde_json::json!({"content":[],"role":"assistant","model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":"bad"}),
        &agent_core::ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderId::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert!(matches!(
        adapter_error.kind,
        AdapterErrorKind::ProtocolViolation | AdapterErrorKind::Decode
    ));
    assert!(!adapter_error.message.is_empty());
}

#[test]
fn anthropic_translator_is_constructible() {
    let _ = base_request();
}
