use reqwest::Method;
use serde_json::json;

use agent_core::types::{AuthStyle, ProtocolKind, ProviderKind, ResponseFormat, ResponseMode};

use crate::{
    adapter::{
        adapter_for,
        tests::shared::{base_task, compose_openai_compatible_request, execution_plan},
    },
    error::AdapterErrorKind,
    families::openai_compatible::wire::{OpenAiDecodeEnvelope, decode::decode_openai_response},
    request_plan::TransportResponseFraming,
};

const OPENAI_MODEL: &str = "openai/gpt-5-mini";

#[test]
fn descriptor_expose_expected_static_metadata() {
    let openai = adapter_for(ProviderKind::OpenAi).descriptor();
    assert_eq!(openai.protocol, ProtocolKind::OpenAI);
    assert_eq!(openai.default_auth_style, AuthStyle::Bearer);
    assert_eq!(openai.default_base_url, "https://api.openai.com");
    assert_eq!(openai.endpoint_path, "/v1/responses");
}

#[test]
fn openai_adapter_plan_request_matches_family_refinement_translation() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::OpenAi,
        &task,
        OPENAI_MODEL,
        ResponseMode::NonStreaming,
        None,
    );

    let translated = compose_openai_compatible_request(
        ProviderKind::OpenAi,
        &task,
        OPENAI_MODEL,
        ResponseMode::NonStreaming,
        None,
    )
    .expect("request planning should succeed");
    let adapter_plan = adapter_for(ProviderKind::OpenAi)
        .plan_request(&execution)
        .expect("adapter planning should succeed");

    assert_eq!(adapter_plan.body, translated.body);
    assert_eq!(adapter_plan.warnings, translated.warnings);
    assert_eq!(adapter_plan.method, Method::POST);
    assert_eq!(
        adapter_plan.response_framing,
        TransportResponseFraming::Json
    );
}

#[test]
fn openai_streaming_plan_preserves_family_default_request_contract() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::OpenAi,
        &task,
        OPENAI_MODEL,
        ResponseMode::Streaming,
        None,
    );
    let adapter_plan = adapter_for(ProviderKind::OpenAi)
        .plan_request(&execution)
        .expect("adapter planning should succeed");

    assert_eq!(adapter_plan.method, Method::POST);
    assert_eq!(adapter_plan.response_framing, TransportResponseFraming::Sse);
    assert!(adapter_plan.endpoint_path_override.is_none());
    assert!(adapter_plan.provider_headers.is_empty());
    let expected = agent_transport::HttpRequestOptions::sse_defaults();
    assert_eq!(
        adapter_plan.request_options.allow_error_status,
        expected.allow_error_status
    );
}

#[test]
fn adapters_decode_responses_with_existing_translators() {
    let format = ResponseFormat::Text;
    let openai_body = json!({
        "status": "completed",
        "model": "gpt-5-mini",
        "output": [{ "type": "message", "content": [{ "type": "output_text", "text": "hello" }] }],
        "usage": { "input_tokens": 1, "output_tokens": 2, "total_tokens": 3 }
    });
    assert_eq!(
        adapter_for(ProviderKind::OpenAi)
            .decode_response_json(openai_body.clone(), &format)
            .expect("decode should succeed"),
        decode_openai_response(&OpenAiDecodeEnvelope {
            body: openai_body,
            requested_response_format: format.clone(),
        })
        .expect("decode should succeed")
    );
}

#[test]
fn adapters_expose_layered_error_decode_contract() {
    let openai_error = adapter_for(ProviderKind::OpenAi)
        .decode_error(&json!({
            "error": {
                "message": "rate limited",
                "code": "rate_limit_exceeded",
                "type": "rate_limit"
            }
        }))
        .expect("error info should decode");
    assert_eq!(
        openai_error.provider_code.as_deref(),
        Some("rate_limit_exceeded")
    );
    assert_eq!(openai_error.kind, Some(AdapterErrorKind::Upstream));
    assert!(
        openai_error
            .message
            .as_deref()
            .is_some_and(|message| message.contains("rate limited"))
    );
}
