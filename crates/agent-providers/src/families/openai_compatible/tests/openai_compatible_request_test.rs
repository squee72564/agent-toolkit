use serde_json::json;

use agent_core::{
    FamilyOptions, Message, OpenAiCompatibleOptions, OpenAiCompatibleReasoning,
    OpenAiCompatibleReasoningEffort, ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
};

use crate::error::AdapterErrorKind;
use crate::interfaces::codec_for;
use crate::request_plan::TransportResponseFraming;
use reqwest::Method;

const MODEL_ID: &str = "gpt-5-mini";

fn base_task() -> TaskRequest {
    TaskRequest {
        messages: vec![Message::user_text("hello")],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
    }
}

#[test]
fn openai_request_plan_uses_json_defaults_for_non_streaming_requests() {
    let plan = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&base_task(), MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    assert_eq!(plan.method, Method::POST);
    assert_eq!(plan.response_framing, TransportResponseFraming::Json);
    assert_eq!(plan.body["model"], "gpt-5-mini");
    assert!(plan.body.get("stream").is_none());
    assert!(plan.request_options.allow_error_status);
}

#[test]
fn openai_request_plan_keeps_semantic_only_tasks_free_of_family_controls() {
    let plan = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&base_task(), MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    assert!(plan.body.get("parallel_tool_calls").is_none());
    assert!(plan.body.get("reasoning").is_none());
    assert!(plan.body.get("temperature").is_none());
    assert!(plan.body.get("top_p").is_none());
    assert!(plan.body.get("max_output_tokens").is_none());
    assert!(plan.warnings.is_empty());
}

#[test]
fn openai_request_plan_enables_sse_for_streaming_requests() {
    let plan = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&base_task(), MODEL_ID, ResponseMode::Streaming, None)
        .expect("planning should succeed");

    assert_eq!(plan.method, Method::POST);
    assert_eq!(plan.response_framing, TransportResponseFraming::Sse);
    assert_eq!(plan.body["stream"], true);
    assert_eq!(
        plan.request_options.expected_content_type.as_deref(),
        Some("text/event-stream")
    );
}

#[test]
fn openai_request_plan_applies_openai_compatible_family_options() {
    let family_options = FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
        parallel_tool_calls: Some(false),
        reasoning: Some(OpenAiCompatibleReasoning {
            effort: Some(OpenAiCompatibleReasoningEffort::Medium),
            summary: None,
        }),
        temperature: Some(1.25),
        top_p: Some(0.85),
        max_output_tokens: Some(256),
    });

    let plan = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(
            &base_task(),
            MODEL_ID,
            ResponseMode::NonStreaming,
            Some(&family_options),
        )
        .expect("planning should succeed");

    assert_eq!(plan.body["parallel_tool_calls"], false);
    assert_eq!(plan.body["reasoning"], json!({ "effort": "medium" }));
    let temperature = plan.body["temperature"]
        .as_f64()
        .expect("temperature should be numeric");
    assert!((temperature - 1.25).abs() < 1e-6);
    let top_p = plan.body["top_p"]
        .as_f64()
        .expect("top_p should be numeric");
    assert!((top_p - 0.85).abs() < 1e-6);
    assert_eq!(plan.body["max_output_tokens"], 256);
}

#[test]
fn openai_request_plan_rejects_out_of_range_temperature() {
    let family_options = FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
        temperature: Some(2.5),
        ..OpenAiCompatibleOptions::default()
    });

    let error = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(
            &base_task(),
            MODEL_ID,
            ResponseMode::NonStreaming,
            Some(&family_options),
        )
        .expect_err("planning should fail");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("temperature"));
}

#[test]
fn openai_request_plan_rejects_out_of_range_top_p() {
    let family_options = FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
        top_p: Some(1.5),
        ..OpenAiCompatibleOptions::default()
    });

    let error = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(
            &base_task(),
            MODEL_ID,
            ResponseMode::NonStreaming,
            Some(&family_options),
        )
        .expect_err("planning should fail");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("top_p"));
}

#[test]
fn openai_request_plan_rejects_zero_max_output_tokens() {
    let family_options = FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
        max_output_tokens: Some(0),
        ..OpenAiCompatibleOptions::default()
    });

    let error = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(
            &base_task(),
            MODEL_ID,
            ResponseMode::NonStreaming,
            Some(&family_options),
        )
        .expect_err("planning should fail");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("max_output_tokens"));
}

#[test]
fn openai_request_plan_rejects_mismatched_family_options() {
    let family_options = FamilyOptions::Anthropic(agent_core::AnthropicFamilyOptions::default());

    let error = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(
            &base_task(),
            MODEL_ID,
            ResponseMode::NonStreaming,
            Some(&family_options),
        )
        .expect_err("planning should fail");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("mismatched family native options"));
}
