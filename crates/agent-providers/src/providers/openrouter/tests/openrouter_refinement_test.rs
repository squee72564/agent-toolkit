use serde_json::json;

use agent_core::{
    ContentPart, FamilyOptions, Message, MessageRole, OpenAiCompatibleOptions, OpenRouterOptions,
    OpenRouterTextOptions, OpenRouterTextVerbosity, ProviderKind, ProviderOptions, ResponseFormat,
    ResponseMode, TaskRequest, ToolChoice,
};

use crate::{
    error::AdapterErrorKind,
    interfaces::{codec_for, refinement_for},
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
        provider_options.map(|options| ProviderOptions::OpenRouter(Box::new(options)));
    refinement_for(agent_core::ProviderKind::OpenRouter).refine_request(
        task,
        model,
        &mut encoded,
        provider_options.as_ref(),
    )?;
    Ok(encoded.into())
}

#[test]
fn openrouter_refinement_preserves_family_encoded_top_p_without_warnings() {
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        Some(OpenAiCompatibleOptions {
            top_p: Some(0.9),
            ..OpenAiCompatibleOptions::default()
        }),
        None,
    )
    .expect("planning should succeed");

    let top_p = encoded.body["top_p"]
        .as_f64()
        .expect("top_p should be numeric");
    assert!((top_p - 0.9).abs() < 1e-6);
    assert!(encoded.warnings.is_empty());
}

#[test]
fn openrouter_refinement_preserves_family_encoded_top_p_with_fallback_models() {
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        Some(OpenAiCompatibleOptions {
            top_p: Some(0.9),
            ..OpenAiCompatibleOptions::default()
        }),
        Some(OpenRouterOptions {
            fallback_models: vec!["openai/gpt-4.1".to_string()],
            ..OpenRouterOptions::default()
        }),
    )
    .expect("planning should succeed");

    assert_eq!(
        encoded.body["models"],
        json!(["openai/gpt-5-mini", "openai/gpt-4.1"])
    );
    let top_p = encoded.body["top_p"]
        .as_f64()
        .expect("top_p should be numeric");
    assert!((top_p - 0.9).abs() < 1e-6);
}

#[test]
fn openrouter_refinement_applies_typed_tier1_and_tier2_overrides() {
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions {
            provider_preferences: Some(json!({ "order": ["openai"] })),
            plugins: vec![json!({ "id": "web" })],
            metadata: std::collections::BTreeMap::from([(
                "trace_id".to_string(),
                "trace-1".to_string(),
            )]),
            top_k: Some(12),
            top_logprobs: Some(3),
            max_tokens: Some(256),
            stop: vec!["DONE".to_string()],
            seed: Some(7),
            logit_bias: std::collections::BTreeMap::from([("42".to_string(), 10)]),
            logprobs: Some(true),
            frequency_penalty: Some(0.25),
            presence_penalty: Some(0.5),
            user: Some("user-1".to_string()),
            session_id: Some("session-1".to_string()),
            trace: Some(json!({ "trace_id": "trace-1" })),
            text: Some(OpenRouterTextOptions {
                verbosity: Some(OpenRouterTextVerbosity::High),
            }),
            modalities: Some(vec!["text".to_string()]),
            image_config: Some(json!({ "size": "1024x1024" })),
            ..OpenRouterOptions::default()
        }),
    )
    .expect("planning should succeed");

    assert_eq!(encoded.body["provider"], json!({ "order": ["openai"] }));
    assert_eq!(encoded.body["plugins"], json!([{ "id": "web" }]));
    assert_eq!(encoded.body["metadata"], json!({ "trace_id": "trace-1" }));
    assert_eq!(encoded.body["top_k"], 12);
    assert_eq!(encoded.body["top_logprobs"], 3);
    assert_eq!(encoded.body["max_tokens"], 256);
    assert_eq!(encoded.body["stop"], json!(["DONE"]));
    assert_eq!(encoded.body["seed"], 7);
    assert_eq!(encoded.body["logit_bias"], json!({ "42": 10 }));
    assert_eq!(encoded.body["logprobs"], true);
    assert_eq!(encoded.body["user"], "user-1");
    assert_eq!(encoded.body["session_id"], "session-1");
    assert_eq!(encoded.body["trace"], json!({ "trace_id": "trace-1" }));
    assert_eq!(encoded.body["text"]["format"]["type"], "text");
    assert_eq!(encoded.body["text"]["verbosity"], "high");
    assert_eq!(encoded.body["modalities"], json!(["text"]));
    assert_eq!(encoded.body["image_config"], json!({ "size": "1024x1024" }));
}

#[test]
fn openrouter_refinement_rejects_top_logprobs_without_logprobs() {
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions {
            top_logprobs: Some(3),
            logprobs: Some(false),
            ..OpenRouterOptions::default()
        }),
    )
    .expect_err("planning should fail without logprobs=true");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("top_logprobs"));
    assert!(error.message.contains("logprobs=true"));
}

#[test]
fn openrouter_refinement_rejects_conflicting_max_tokens_controls() {
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        Some(OpenAiCompatibleOptions {
            max_output_tokens: Some(128),
            ..OpenAiCompatibleOptions::default()
        }),
        Some(OpenRouterOptions {
            max_tokens: Some(64),
            ..OpenRouterOptions::default()
        }),
    )
    .expect_err("planning should fail for conflicting token controls");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("max_tokens"));
    assert!(error.message.contains("max_output_tokens"));
}

#[test]
fn openrouter_refinement_rejects_invalid_metadata() {
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions {
            metadata: std::collections::BTreeMap::from([(
                "bad[key]".to_string(),
                "value".to_string(),
            )]),
            ..OpenRouterOptions::default()
        }),
    )
    .expect_err("planning should fail for invalid metadata");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("metadata"));
    assert!(error.message.contains("brackets"));
}

#[test]
fn openrouter_refinement_rejects_invalid_logit_bias_range() {
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions {
            logit_bias: std::collections::BTreeMap::from([("42".to_string(), 101)]),
            ..OpenRouterOptions::default()
        }),
    )
    .expect_err("planning should fail for out of range logit_bias");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("logit_bias"));
}

#[test]
fn openrouter_refinement_rejects_invalid_modality() {
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions {
            modalities: Some(vec!["audio".to_string()]),
            ..OpenRouterOptions::default()
        }),
    )
    .expect_err("planning should fail for unsupported modality");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("modalities"));
}

#[test]
fn openrouter_refinement_rejects_non_finite_frequency_penalty_override() {
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions {
            frequency_penalty: Some(f32::NAN),
            ..OpenRouterOptions::default()
        }),
    )
    .expect_err("planning should fail for non-finite frequency_penalty");

    assert!(error.message.contains("frequency_penalty"));
    assert!(error.message.contains("must be finite"));
}

#[test]
fn openrouter_refinement_rejects_non_finite_presence_penalty_override() {
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
        Some(OpenRouterOptions {
            presence_penalty: Some(f32::INFINITY),
            ..OpenRouterOptions::default()
        }),
    )
    .expect_err("planning should fail for non-finite presence_penalty");

    assert!(error.message.contains("presence_penalty"));
    assert!(error.message.contains("must be finite"));
}

#[test]
fn openrouter_refinement_rejects_mismatched_provider_options() {
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&base_task(), MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    let error = refinement_for(agent_core::ProviderKind::OpenRouter)
        .refine_request(
            &base_task(),
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::OpenAi(
                agent_core::OpenAiOptions::default(),
            )),
        )
        .expect_err("planning should reject mismatched provider options");

    assert_eq!(error.provider, ProviderKind::OpenRouter);
    assert_eq!(error.kind, AdapterErrorKind::Validation);
}
