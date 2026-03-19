use reqwest::Method;
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::json;

use agent_core::types::{
    AuthStyle, NativeOptions, ProtocolKind, ProviderKind, ResponseFormat, ResponseMode, TaskRequest,
};

use crate::adapter::adapter_for;
use crate::adapter::tests::shared::{base_task, execution_plan};
use crate::anthropic_family::AnthropicDecodeEnvelope;
use crate::anthropic_family::decode::decode_anthropic_response;
use crate::error::AdapterErrorKind;
use crate::family_codec::codec_for;
use crate::refinement::refinement_for;
use crate::request_plan::TransportResponseFraming;

const ANTHROPIC_MODEL: &str = "claude-sonnet-4-6";

fn compose_anthropic_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
    native_options: Option<&NativeOptions>,
) -> Result<crate::request_plan::ProviderRequestPlan, crate::error::AdapterError> {
    let codec = codec_for(agent_core::ProviderFamilyId::Anthropic);
    let refinement = refinement_for(ProviderKind::Anthropic);
    let mut encoded = codec.encode_task(
        task,
        model,
        response_mode,
        native_options.and_then(|native| native.family.as_ref()),
    )?;
    refinement.refine_request(
        task,
        model,
        &mut encoded,
        native_options.and_then(|native| native.provider.as_ref()),
    )?;
    Ok(encoded.into())
}

#[test]
fn descriptors_expose_expected_static_metadata() {
    let anthropic = adapter_for(ProviderKind::Anthropic).descriptor();
    assert_eq!(anthropic.protocol, ProtocolKind::Anthropic);
    assert_eq!(
        anthropic.default_auth_style,
        AuthStyle::ApiKeyHeader(HeaderName::from_static("x-api-key"))
    );
    assert_eq!(
        anthropic
            .default_headers
            .get(HeaderName::from_static("anthropic-version")),
        Some(&HeaderValue::from_static("2023-06-01"))
    );
}

#[test]
fn anthropic_adapter_plan_request_matches_family_overlay_translation() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::Anthropic,
        &task,
        ANTHROPIC_MODEL,
        ResponseMode::NonStreaming,
        None,
    );

    let translated =
        compose_anthropic_request(&task, ANTHROPIC_MODEL, ResponseMode::NonStreaming, None)
            .expect("planning should succeed");
    let adapter_plan = adapter_for(ProviderKind::Anthropic)
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
fn anthropic_non_streaming_plan_preserves_family_default_request_contract() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::Anthropic,
        &task,
        ANTHROPIC_MODEL,
        ResponseMode::NonStreaming,
        None,
    );
    let adapter_plan = adapter_for(ProviderKind::Anthropic)
        .plan_request(&execution)
        .expect("adapter planning should succeed");

    assert_eq!(adapter_plan.method, Method::POST);
    assert_eq!(
        adapter_plan.response_framing,
        TransportResponseFraming::Json
    );
    assert!(adapter_plan.endpoint_path_override.is_none());
    assert!(adapter_plan.provider_headers.is_empty());
    assert!(adapter_plan.request_options.allow_error_status);
}

#[test]
fn adapters_decode_responses_with_existing_translators() {
    let format = ResponseFormat::Text;
    let anthropic_body = json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "model": "claude-sonnet-4-6",
        "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": "hello" }],
        "usage": { "input_tokens": 1, "output_tokens": 2 }
    });
    assert_eq!(
        adapter_for(ProviderKind::Anthropic)
            .decode_response_json(anthropic_body.clone(), &format)
            .expect("decode should succeed"),
        decode_anthropic_response(&AnthropicDecodeEnvelope {
            body: anthropic_body,
            requested_response_format: format.clone(),
        })
        .expect("decode should succeed")
    );
}

#[test]
fn adapters_expose_layered_error_decode_contract() {
    let anthropic_error = adapter_for(ProviderKind::Anthropic)
        .decode_error(&json!({
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "message": "bad input"
            },
            "request_id": "req_123"
        }))
        .expect("error info should decode");
    assert_eq!(
        anthropic_error.provider_code.as_deref(),
        Some("invalid_request_error")
    );
    assert_eq!(anthropic_error.kind, Some(AdapterErrorKind::Upstream));
    assert!(
        anthropic_error
            .message
            .as_deref()
            .is_some_and(|message| message.contains("bad input"))
    );
}
