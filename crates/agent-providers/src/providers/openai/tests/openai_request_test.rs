use std::collections::BTreeMap;

use agent_core::types::{
    ContentPart, FamilyOptions, Message, MessageRole, OpenAiCompatibleOptions, OpenAiOptions,
    OpenAiPromptCacheRetention, OpenAiTextOptions, OpenAiTextVerbosity, OpenAiTruncation,
    ProviderKind, ProviderOptions, ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
};

use crate::error::{AdapterErrorKind, AdapterOperation};
use crate::interfaces::codec_for;
use crate::interfaces::refinement_for;

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
    }
}

fn assert_json_number_close(actual: &serde_json::Value, expected: f64) {
    let actual = actual
        .as_f64()
        .expect("expected JSON number in encoded request body");
    let delta = (actual - expected).abs();
    assert!(
        delta < 1e-6,
        "expected numeric value close to {expected}, got {actual} (delta {delta})"
    );
}

fn plan_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
    family_options: Option<&FamilyOptions>,
    provider_options: Option<&ProviderOptions>,
) -> Result<crate::request_plan::ProviderRequestPlan, crate::error::AdapterError> {
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible).encode_task(
        task,
        model,
        response_mode,
        family_options,
    )?;
    refinement_for(ProviderKind::OpenAi).refine_request(
        task,
        model,
        &mut encoded,
        provider_options,
    )?;
    Ok(encoded.into())
}

#[test]
fn openai_request_error_maps_into_adapter_error() {
    let adapter_error = plan_request(&base_task(), "", ResponseMode::NonStreaming, None, None)
        .expect_err("planning should fail");

    assert_eq!(adapter_error.provider, ProviderKind::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::PlanRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert!(!adapter_error.message.is_empty());
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn openai_request_error_preserves_source_chain() {
    let adapter_error = plan_request(&base_task(), "", ResponseMode::NonStreaming, None, None)
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
fn openai_request_plan_passes_through_openai_encoder() {
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        None,
    )
    .expect("planning should succeed");

    assert_eq!(encoded.body["model"], MODEL_ID);
    assert!(encoded.body["input"].is_array());
}

#[test]
fn openai_request_plan_does_not_encode_provider_controls_without_provider_options() {
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        None,
    )
    .expect("planning should succeed");

    assert!(encoded.body.get("metadata").is_none());
    assert!(encoded.body.get("store").is_none());
    assert!(encoded.body.get("service_tier").is_none());
    assert!(encoded.body.get("prompt_cache_key").is_none());
    assert!(encoded.body.get("prompt_cache_retention").is_none());
    assert!(encoded.body.get("truncation").is_none());
    assert!(encoded.body.get("safety_identifier").is_none());
    assert!(encoded.body["text"].get("verbosity").is_none());
}

#[test]
fn openai_request_plan_applies_provider_native_options_in_refinement() {
    let provider_options = ProviderOptions::OpenAi(OpenAiOptions {
        metadata: BTreeMap::from([("trace_id".to_string(), "trace-1".to_string())]),
        service_tier: Some("flex".to_string()),
        store: Some(true),
        prompt_cache_key: Some("cache-key-1".to_string()),
        prompt_cache_retention: Some(OpenAiPromptCacheRetention::InMemory),
        truncation: Some(OpenAiTruncation::Auto),
        text: Some(OpenAiTextOptions {
            verbosity: Some(OpenAiTextVerbosity::Medium),
        }),
        safety_identifier: Some("safe-1".to_string()),
    });

    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(&provider_options),
    )
    .expect("planning should succeed");

    assert_eq!(encoded.body["metadata"]["trace_id"], "trace-1");
    assert_eq!(encoded.body["service_tier"], "flex");
    assert_eq!(encoded.body["store"], true);
    assert_eq!(encoded.body["prompt_cache_key"], "cache-key-1");
    assert_eq!(encoded.body["prompt_cache_retention"], "in-memory");
    assert_eq!(encoded.body["truncation"], "auto");
    assert_eq!(encoded.body["text"]["verbosity"], "medium");
    assert_eq!(encoded.body["text"]["format"]["type"], "text");
    assert_eq!(encoded.body["safety_identifier"], "safe-1");
}

#[test]
fn openai_request_plan_accepts_family_and_provider_options_without_mutating_task() {
    let family_options = FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
        temperature: Some(0.6),
        top_p: Some(0.7),
        max_output_tokens: Some(128),
        ..OpenAiCompatibleOptions::default()
    });
    let provider_options = ProviderOptions::OpenAi(OpenAiOptions {
        metadata: BTreeMap::from([("trace_id".to_string(), "trace-2".to_string())]),
        ..OpenAiOptions::default()
    });

    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        Some(&family_options),
        Some(&provider_options),
    )
    .expect("planning should succeed");

    assert_json_number_close(&encoded.body["temperature"], 0.6);
    assert_json_number_close(&encoded.body["top_p"], 0.7);
    assert_eq!(encoded.body["max_output_tokens"], 128);
    assert_eq!(encoded.body["metadata"]["trace_id"], "trace-2");
}
