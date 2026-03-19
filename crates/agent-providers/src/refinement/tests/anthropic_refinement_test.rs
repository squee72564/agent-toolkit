use crate::error::{AdapterErrorKind, AdapterOperation};
use crate::family_codec::codec_for;
use crate::refinement::refinement_for;
use agent_core::{
    AnthropicOptions, ContentPart, Message, MessageRole, OpenAiOptions, ProviderOptions,
    ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
};

const MODEL_ID: &str = "claude-sonnet-4-6";

fn base_task() -> TaskRequest {
    TaskRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::text("hello")],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: std::collections::BTreeMap::new(),
    }
}

#[test]
fn anthropic_refinement_applies_provider_native_options() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::Anthropic)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    refinement_for(agent_core::ProviderKind::Anthropic)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::Anthropic(AnthropicOptions {
                top_k: Some(8),
            })),
        )
        .expect("refinement should succeed");

    assert_eq!(encoded.body["top_k"], 8);
}

#[test]
fn anthropic_refinement_rejects_mismatched_provider_options() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::Anthropic)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    let error = refinement_for(agent_core::ProviderKind::Anthropic)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::OpenAi(OpenAiOptions {
                service_tier: Some("flex".to_string()),
                store: Some(false),
            })),
        )
        .expect_err("refinement should reject mismatched options");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert!(error.message.contains("mismatched provider native options"));
}
