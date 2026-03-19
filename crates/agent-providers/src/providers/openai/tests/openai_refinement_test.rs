use std::collections::BTreeMap;

use agent_core::{
    ContentPart, Message, MessageRole, OpenAiOptions, ProviderOptions, ResponseFormat,
    ResponseMode, TaskRequest, ToolChoice,
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
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

#[test]
fn openai_refinement_applies_provider_native_options() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    refinement_for(agent_core::ProviderKind::OpenAi)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::OpenAi(OpenAiOptions {
                service_tier: Some("flex".to_string()),
                store: Some(true),
            })),
        )
        .expect("refinement should succeed");

    assert_eq!(encoded.body["service_tier"], "flex");
    assert_eq!(encoded.body["store"], true);
}

#[test]
fn openai_refinement_rejects_mismatched_provider_options() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    let error = refinement_for(agent_core::ProviderKind::OpenAi)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::Anthropic(agent_core::AnthropicOptions {
                top_k: Some(8),
            })),
        )
        .expect_err("refinement should reject mismatched options");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert!(error.message.contains("mismatched provider native options"));
}
