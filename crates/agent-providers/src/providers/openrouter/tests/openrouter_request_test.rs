use reqwest::Method;

use agent_core::{
    ContentPart, FamilyOptions, Message, MessageRole, OpenAiCompatibleOptions, OpenRouterOptions,
    ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
};

use crate::{
    interfaces::{codec_for, refinement_for},
    request_plan::TransportResponseFraming,
};

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
    }
}

fn plan_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
    family_options: Option<OpenAiCompatibleOptions>,
    provider_options: Option<OpenRouterOptions>,
) -> Result<crate::request_plan::ProviderRequestPlan, crate::error::AdapterError> {
    let family_options = family_options
        .as_ref()
        .map(|options| FamilyOptions::OpenAiCompatible(options.clone()));
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(task, model, response_mode, family_options.as_ref())
        .map_err(|mut error| {
            error.provider = agent_core::ProviderKind::OpenRouter;
            error
        })?;
    let provider_options =
        provider_options.map(|options| agent_core::ProviderOptions::OpenRouter(Box::new(options)));
    refinement_for(agent_core::ProviderKind::OpenRouter).refine_request(
        task,
        model,
        &mut encoded,
        provider_options.as_ref(),
    )?;
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
    let adapter_error = plan_request(&base_task(), "", ResponseMode::NonStreaming, None, None)
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
fn openrouter_request_reuses_openai_family_encoder() {
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions {
            max_tokens: Some(256),
            stop: vec!["DONE".to_string()],
            ..OpenRouterOptions::default()
        }),
    )
    .expect("planning should succeed");

    assert_eq!(encoded.body["model"], MODEL_ID);
    assert!(encoded.body["input"].is_array());
    assert_eq!(encoded.body["max_tokens"], 256);
    assert_eq!(encoded.body["stop"], serde_json::json!(["DONE"]));
}

#[test]
fn openrouter_request_does_not_reintroduce_provider_fields_from_semantic_task_input() {
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions::default()),
    )
    .expect("planning should succeed");

    assert!(encoded.body.get("metadata").is_none());
    assert!(encoded.body.get("top_k").is_none());
    assert!(encoded.body.get("top_logprobs").is_none());
    assert!(encoded.body.get("max_tokens").is_none());
    assert!(encoded.body.get("stop").is_none());
    assert!(encoded.body.get("seed").is_none());
    assert!(encoded.body.get("logit_bias").is_none());
    assert!(encoded.body.get("logprobs").is_none());
    assert!(encoded.body.get("provider").is_none());
    assert!(encoded.body.get("plugins").is_none());
}
