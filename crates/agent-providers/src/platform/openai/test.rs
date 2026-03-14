use std::collections::BTreeMap;

use crate::adapter::adapter_for;
use crate::error::{AdapterErrorKind, AdapterOperation};
use agent_core::types::ProviderKind;
use agent_core::types::{
    ContentPart, Message, MessageRole, ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
};
use serde_json::json;

use super::request;

const MODEL_ID: &str = "gpt-4.1-mini";

fn base_task() -> TaskRequest {
    TaskRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "hello".to_string(),
            }],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

#[test]
fn openai_request_error_maps_into_adapter_error() {
    let adapter_error = request::plan_request(
        &base_task(),
        "",
        ResponseMode::NonStreaming,
        ProviderKind::OpenAi,
        None,
    )
    .expect_err("planning should fail");

    assert_eq!(adapter_error.provider, ProviderKind::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::PlanRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert!(!adapter_error.message.is_empty());
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn openai_response_error_maps_into_adapter_error() {
    let adapter_error = adapter_for(ProviderKind::OpenAi)
        .decode_response_json(json!("bad response"), &ResponseFormat::Text)
        .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderKind::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert!(!adapter_error.message.is_empty());
}

#[test]
fn openai_request_error_preserves_source_chain() {
    let adapter_error = request::plan_request(
        &base_task(),
        "",
        ResponseMode::NonStreaming,
        ProviderKind::OpenAi,
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
fn openai_upstream_error_maps_into_adapter_error() {
    let adapter_error = adapter_for(ProviderKind::OpenAi)
        .decode_response_json(
            json!({"error":{"message":"provider said no","code":"rate_limit_exceeded","type":"rate_limit_error"}}),
            &ResponseFormat::Text,
        )
        .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderKind::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert!(adapter_error.message.contains("provider said no"));
    assert_eq!(
        adapter_error.provider_code.as_deref(),
        Some("rate_limit_exceeded")
    );
}

#[test]
fn openai_decode_empty_content_is_nonfatal_and_warns() {
    let response = adapter_for(ProviderKind::OpenAi)
        .decode_response_json(
            json!({"status":"completed","model":"gpt-4.1-mini","output":"not-an-array"}),
            &ResponseFormat::Text,
        )
        .expect("decode should succeed with warning");

    assert_eq!(response.model, "gpt-4.1-mini");
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "openai.decode.empty_content")
    );
}

#[test]
fn openai_translator_is_constructible() {
    let _ = base_task();
}

#[test]
fn openai_request_plan_passes_through_openai_encoder() {
    let encoded = request::plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        ProviderKind::OpenAi,
        None,
    )
    .expect("planning should succeed");

    assert_eq!(encoded.body["model"], MODEL_ID);
    assert!(encoded.body["input"].is_array());
}

#[test]
fn openai_response_decode_passes_through_openai_decoder() {
    let response = adapter_for(ProviderKind::OpenAi)
        .decode_response_json(
            json!({
                "status": "completed",
                "model": "gpt-4.1-mini",
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

    assert_eq!(response.model, "gpt-4.1-mini");
    assert_eq!(
        response.output.content,
        vec![ContentPart::Text {
            text: "hello from openai format".to_string()
        }]
    );
    assert_eq!(response.usage.input_tokens, Some(3));
    assert_eq!(response.usage.output_tokens, Some(4));
    assert_eq!(response.usage.total_tokens, Some(7));
}
