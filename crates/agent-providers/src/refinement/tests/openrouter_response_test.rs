use agent_core::ResponseFormat;
use serde_json::json;

use crate::adapter::adapter_for;
use crate::error::{AdapterErrorKind, AdapterOperation};
use agent_core::ContentPart;
use agent_core::ProviderKind;

#[test]
fn openrouter_response_decoder_rejects_chat_completions_payloads() {
    let error = decode_response_json(
        json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-5-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": "hello"
                }
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 6,
                "total_tokens": 11
            }
        }),
        &ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert!(!error.message.is_empty());
}

#[test]
fn openrouter_upstream_error_maps_into_adapter_error() {
    let adapter_error = decode_response_json(
        json!({"error":{"message":"provider failure","code":"rate_limit_exceeded"}}),
        &ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderKind::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert!(adapter_error.message.contains("provider failure"));
    assert_eq!(
        adapter_error.provider_code.as_deref(),
        Some("rate_limit_exceeded")
    );
}

#[test]
fn openrouter_decode_error_maps_into_adapter_error() {
    let adapter_error = decode_response_json(json!("bad response"), &ResponseFormat::Text)
        .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderKind::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert!(!adapter_error.message.is_empty());
}

#[test]
fn openrouter_protocol_violation_error_maps_into_adapter_error() {
    let adapter_error = decode_response_json(
        json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-4.1-mini",
            "choices": "bad"
        }),
        &ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderKind::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert!(!adapter_error.message.is_empty());
}

#[test]
fn openrouter_decode_uses_openai_path_when_payload_is_openai_compatible() {
    let response = decode_response_json(
        json!({
            "status": "completed",
            "model": "openai/gpt-4.1-mini",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "hello from openai format"
                }]
            }],
            "usage": {
                "input_tokens": 3,
                "output_tokens": 4,
                "total_tokens": 7
            }
        }),
        &ResponseFormat::Text,
    )
    .expect("decode should succeed");

    assert_eq!(response.model, "openai/gpt-4.1-mini");
    assert_eq!(
        response.output.content,
        vec![ContentPart::Text {
            text: "hello from openai format".to_string()
        }]
    );
    assert!(response.warnings.is_empty());
}

#[test]
fn openrouter_decode_maps_upstream_error_without_fallback_context() {
    let error = decode_response_json(
        json!({
            "error": {
                "message": "upstream hard failure",
                "code": 401
            }
        }),
        &ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert_eq!(error.kind, AdapterErrorKind::Upstream);
    assert!(error.message.contains("upstream hard failure"));
    assert!(!error.message.contains("fallback"));
}

fn decode_response_json(
    body: serde_json::Value,
    requested_format: &ResponseFormat,
) -> Result<agent_core::Response, crate::error::AdapterError> {
    adapter_for(ProviderKind::OpenRouter).decode_response_json(body, requested_format)
}
