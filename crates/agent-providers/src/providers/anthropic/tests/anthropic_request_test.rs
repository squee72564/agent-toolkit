use crate::error::{AdapterErrorKind, AdapterOperation};
use crate::interfaces::codec_for;
use crate::interfaces::refinement_for;
use agent_core::{
    AnthropicFamilyOptions, AnthropicOptions, AnthropicThinking, AnthropicToolChoiceOptions,
    ContentPart, FamilyOptions, Message, MessageRole, ProviderKind, ProviderOptions,
    ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
};

fn base_task() -> TaskRequest {
    TaskRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::text("hello")],
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
    family_options: Option<&FamilyOptions>,
    provider_options: Option<&ProviderOptions>,
) -> Result<crate::request_plan::ProviderRequestPlan, crate::error::AdapterError> {
    let mut encoded = codec_for(agent_core::ProviderFamilyId::Anthropic).encode_task(
        task,
        model,
        response_mode,
        family_options,
    )?;
    refinement_for(ProviderKind::Anthropic).refine_request(
        task,
        model,
        &mut encoded,
        provider_options,
    )?;
    Ok(encoded.into())
}

#[test]
fn anthropic_request_error_maps_into_adapter_error() {
    let adapter_error = plan_request(&base_task(), "", ResponseMode::NonStreaming, None, None)
        .expect_err("planning should fail");

    assert_eq!(adapter_error.provider, ProviderKind::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::PlanRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert!(!adapter_error.message.is_empty());
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn anthropic_request_error_preserves_source_chain() {
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
fn anthropic_request_plan_accepts_family_options_without_mutating_task() {
    let family_options = FamilyOptions::Anthropic(AnthropicFamilyOptions {
        thinking: Some(AnthropicThinking::Disabled),
    });

    let provider_options = ProviderOptions::Anthropic(AnthropicOptions {
        temperature: Some(0.2),
        max_tokens: Some(1024),
        tool_choice: Some(AnthropicToolChoiceOptions::Auto {
            disable_parallel_tool_use: Some(true),
        }),
        ..Default::default()
    });

    let task_clone = base_task().clone();

    let encoded = plan_request(
        &base_task(),
        "claude-sonnet-4-6",
        ResponseMode::NonStreaming,
        Some(&family_options),
        Some(&provider_options),
    )
    .expect("planning should succeed");

    assert_eq!(task_clone, base_task());

    assert_eq!(encoded.body["thinking"]["type"], "disabled");
    assert!(encoded.body.get("temperature").is_some());
    assert!(encoded.body.get("max_tokens").is_some());
}

#[test]
fn semantic_only_request_omits_max_tokens() {
    let encoded = plan_request(
        &base_task(),
        "claude-sonnet-4-6",
        ResponseMode::NonStreaming,
        None,
        None,
    )
    .expect("planning should succeed");

    assert!(encoded.body.get("max_tokens").is_none());
}

#[test]
fn anthropic_request_plan_rejects_enabled_thinking_without_max_tokens() {
    let family_options = FamilyOptions::Anthropic(AnthropicFamilyOptions {
        thinking: Some(AnthropicThinking::Enabled {
            budget_tokens: agent_core::AnthropicThinkingBudget::new(1024)
                .expect("non-zero thinking budget"),
            display: None,
        }),
    });

    let error = plan_request(
        &base_task(),
        "claude-sonnet-4-6",
        ResponseMode::NonStreaming,
        Some(&family_options),
        None,
    )
    .expect_err("planning should reject enabled thinking without max_tokens");

    assert!(error.message.contains("requires max_tokens"));
}
