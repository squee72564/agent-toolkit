use std::collections::BTreeMap;

use agent_core::{
    ContentPart, Message, MessageRole, ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
};

use crate::family_codec::codec_for;
use crate::refinement::openrouter::{
    OpenRouterOverrides, apply_openrouter_overrides,
};
use crate::refinement::refinement_for;
use crate::request_plan::TransportResponseFraming;
use reqwest::Method;

const MODEL_ID: &str = "openai/gpt-5-mini";

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

fn plan_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
    overrides: &OpenRouterOverrides,
) -> Result<crate::request_plan::ProviderRequestPlan, crate::error::AdapterError> {
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(task, model, response_mode, None)
        .map_err(|mut error| {
            error.provider = agent_core::ProviderKind::OpenRouter;
            error
        })?;
    apply_openrouter_overrides(
        model,
        task.top_p,
        &task.stop,
        overrides,
        &mut encoded.body,
        &mut encoded.warnings,
    )
    .map_err(|error| {
        crate::error::AdapterError::with_source(
            match error.kind() {
                crate::openai_family::OpenAiFamilyErrorKind::Validation => {
                    crate::error::AdapterErrorKind::Validation
                }
                crate::openai_family::OpenAiFamilyErrorKind::Encode => {
                    crate::error::AdapterErrorKind::Encode
                }
                crate::openai_family::OpenAiFamilyErrorKind::Decode => {
                    crate::error::AdapterErrorKind::Decode
                }
                crate::openai_family::OpenAiFamilyErrorKind::Upstream => {
                    crate::error::AdapterErrorKind::Upstream
                }
                crate::openai_family::OpenAiFamilyErrorKind::ProtocolViolation => {
                    crate::error::AdapterErrorKind::ProtocolViolation
                }
                crate::openai_family::OpenAiFamilyErrorKind::UnsupportedFeature => {
                    crate::error::AdapterErrorKind::UnsupportedFeature
                }
            },
            agent_core::ProviderKind::OpenRouter,
            crate::error::AdapterOperation::PlanRequest,
            error.message().to_string(),
            error,
        )
    })?;
    Ok(encoded.into())
}

#[test]
fn openrouter_request_plan_uses_json_defaults_for_non_streaming_requests() {
    let mut plan = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&base_task(), MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");
    refinement_for(agent_core::ProviderKind::OpenRouter)
        .refine_request(
            &base_task(),
            MODEL_ID,
            &mut plan,
            Some(&agent_core::ProviderOptions::OpenRouter(Box::default())),
        )
        .expect("planning should succeed");

    assert_eq!(plan.method, Method::POST);
    assert_eq!(plan.response_framing, TransportResponseFraming::Json);
    assert!(plan.body.get("stream").is_none());
    assert!(plan.request_options.allow_error_status);
}

#[test]
fn openrouter_request_plan_enables_sse_for_streaming_requests() {
    let mut plan = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&base_task(), MODEL_ID, ResponseMode::Streaming, None)
        .expect("planning should succeed");
    refinement_for(agent_core::ProviderKind::OpenRouter)
        .refine_request(
            &base_task(),
            MODEL_ID,
            &mut plan,
            Some(&agent_core::ProviderOptions::OpenRouter(Box::default())),
        )
        .expect("planning should succeed");

    assert_eq!(plan.method, Method::POST);
    assert_eq!(plan.response_framing, TransportResponseFraming::Sse);
    assert_eq!(plan.body["stream"], true);
}

#[test]
fn openrouter_request_error_maps_into_adapter_error() {
    let adapter_error = plan_request(
        &base_task(),
        "",
        ResponseMode::NonStreaming,
        &OpenRouterOverrides::default(),
    )
    .expect_err("planning should fail");

    assert_eq!(adapter_error.provider, agent_core::ProviderKind::OpenRouter);
    assert_eq!(
        adapter_error.operation,
        crate::error::AdapterOperation::PlanRequest
    );
    assert_eq!(
        adapter_error.kind,
        crate::error::AdapterErrorKind::Validation
    );
    assert!(adapter_error.message.contains("model_id must not be empty"));
}

#[test]
fn openrouter_request_error_preserves_source_chain() {
    let adapter_error = plan_request(
        &base_task(),
        "",
        ResponseMode::NonStreaming,
        &OpenRouterOverrides::default(),
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
fn openrouter_request_reuses_openai_family_encoder() {
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        &OpenRouterOverrides::default(),
    )
    .expect("planning should succeed");

    assert_eq!(encoded.body["model"], MODEL_ID);
    assert!(encoded.body["input"].is_array());
}
