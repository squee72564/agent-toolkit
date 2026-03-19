use std::collections::BTreeMap;

use agent_core::{
    ContentPart, Message, MessageRole, ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
};
use serde_json::{Map, json};

use crate::interfaces::codec_for;
use crate::refinement::openrouter::{
    OpenRouterOverrides, apply_openrouter_overrides,
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
fn openrouter_refinement_preserves_openai_encode_warnings() {
    let mut task = base_task();
    task.top_p = Some(0.9);
    task.stop = vec!["DONE".to_string()];

    let encoded = plan_request(
        &task,
        MODEL_ID,
        ResponseMode::NonStreaming,
        &OpenRouterOverrides::default(),
    )
    .expect("planning should succeed");

    let top_p = encoded.body["top_p"]
        .as_f64()
        .expect("top_p should be numeric");
    assert!((top_p - 0.9).abs() < 1e-6);
    assert_eq!(encoded.body["stop"], json!(["DONE"]));
    assert!(
        encoded
            .warnings
            .iter()
            .all(|w| w.code != "openai.encode.ignored_top_p"),
    );
    assert!(
        encoded
            .warnings
            .iter()
            .all(|w| w.code != "openai.encode.ignored_stop"),
    );
}

#[test]
fn openrouter_refinement_reintroduces_top_p_and_stop_with_fallback_models() {
    let overrides = OpenRouterOverrides {
        fallback_models: vec!["openai/gpt-4.1".to_string()],
        ..OpenRouterOverrides::default()
    };
    let mut task = base_task();
    task.top_p = Some(0.9);
    task.stop = vec!["DONE".to_string()];

    let encoded = plan_request(&task, MODEL_ID, ResponseMode::NonStreaming, &overrides)
        .expect("planning should succeed");

    assert_eq!(
        encoded.body["models"],
        json!(["openai/gpt-5-mini", "openai/gpt-4.1"])
    );
    let top_p = encoded.body["top_p"]
        .as_f64()
        .expect("top_p should be numeric");
    assert!((top_p - 0.9).abs() < 1e-6);
    assert_eq!(encoded.body["stop"], json!(["DONE"]));
}

#[test]
fn openrouter_refinement_applies_typed_overrides() {
    let mut task = base_task();
    task.max_output_tokens = Some(384);
    let overrides = OpenRouterOverrides {
        user: Some("user-1".to_string()),
        route: Some("fallback".to_string()),
        ..OpenRouterOverrides::default()
    };
    let encoded = plan_request(&task, MODEL_ID, ResponseMode::NonStreaming, &overrides)
        .expect("planning should succeed");

    assert_eq!(encoded.body["max_output_tokens"], 384);
    assert_eq!(encoded.body["user"], "user-1");
    assert_eq!(encoded.body["route"], "fallback");
}

#[test]
fn openrouter_refinement_omits_empty_serde_backed_overrides() {
    let overrides = OpenRouterOverrides {
        plugins: Vec::new(),
        modalities: Some(Vec::new()),
        ..OpenRouterOverrides::default()
    };
    let encoded = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        &overrides,
    )
    .expect("planning should succeed");

    assert!(encoded.body.get("plugins").is_none());
    assert_eq!(encoded.body["modalities"], json!([]));
    assert!(encoded.body.get("provider").is_none());
    assert!(encoded.body.get("user").is_none());
}

#[test]
fn openrouter_refinement_rejects_non_finite_frequency_penalty_override() {
    let overrides = OpenRouterOverrides {
        frequency_penalty: Some(f32::NAN),
        ..OpenRouterOverrides::default()
    };
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        &overrides,
    )
    .expect_err("planning should fail for non-finite frequency_penalty");
    assert!(error.message.contains("frequency_penalty"));
    assert!(error.message.contains("must be finite"));
}

#[test]
fn openrouter_refinement_rejects_non_finite_presence_penalty_override() {
    let overrides = OpenRouterOverrides {
        presence_penalty: Some(f32::INFINITY),
        ..OpenRouterOverrides::default()
    };
    let error = plan_request(
        &base_task(),
        MODEL_ID,
        ResponseMode::NonStreaming,
        &overrides,
    )
    .expect_err("planning should fail for non-finite presence_penalty");
    assert!(error.message.contains("presence_penalty"));
    assert!(error.message.contains("must be finite"));
}

#[test]
fn openrouter_refinement_extra_overrides_take_precedence() {
    let mut extra = Map::new();
    extra.insert("user".to_string(), json!("from-extra"));
    extra.insert("max_tokens".to_string(), json!(777));

    let mut task = base_task();
    task.max_output_tokens = Some(111);
    let overrides = OpenRouterOverrides {
        user: Some("from-typed".to_string()),
        extra,
        ..OpenRouterOverrides::default()
    };
    let encoded = plan_request(&task, MODEL_ID, ResponseMode::NonStreaming, &overrides)
        .expect("planning should succeed");

    assert_eq!(encoded.body["user"], "from-extra");
    assert_eq!(encoded.body["max_tokens"], 777);
}
