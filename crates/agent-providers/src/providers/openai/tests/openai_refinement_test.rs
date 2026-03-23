use std::collections::BTreeMap;

use agent_core::{
    ContentPart, Message, MessageRole, OpenAiOptions, OpenAiPromptCacheRetention,
    OpenAiTextOptions, OpenAiTextVerbosity, OpenAiTruncation, ProviderOptions, ResponseFormat,
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
                metadata: BTreeMap::from([("trace_id".to_string(), "trace-1".to_string())]),
                service_tier: Some("flex".to_string()),
                store: Some(true),
                prompt_cache_key: Some("cache-key-1".to_string()),
                prompt_cache_retention: Some(OpenAiPromptCacheRetention::TwentyFourHours),
                truncation: Some(OpenAiTruncation::Disabled),
                text: Some(OpenAiTextOptions {
                    verbosity: Some(OpenAiTextVerbosity::High),
                }),
                safety_identifier: Some("safe-1".to_string()),
            })),
        )
        .expect("refinement should succeed");

    assert_eq!(encoded.body["metadata"]["trace_id"], "trace-1");
    assert_eq!(encoded.body["service_tier"], "flex");
    assert_eq!(encoded.body["store"], true);
    assert_eq!(encoded.body["prompt_cache_key"], "cache-key-1");
    assert_eq!(encoded.body["prompt_cache_retention"], "24h");
    assert_eq!(encoded.body["truncation"], "disabled");
    assert_eq!(encoded.body["text"]["verbosity"], "high");
    assert_eq!(encoded.body["text"]["format"]["type"], "text");
    assert_eq!(encoded.body["safety_identifier"], "safe-1");
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
                temperature: None,
                top_p: None,
                max_tokens: None,
                top_k: Some(8),
                stop_sequences: Vec::new(),
                metadata_user_id: None,
                output_config: None,
                service_tier: None,
                tool_choice: None,
                inference_geo: None,
            })),
        )
        .expect_err("refinement should reject mismatched options");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert!(error.message.contains("mismatched provider native options"));
}

#[test]
fn openai_refinement_rejects_metadata_with_more_than_sixteen_pairs() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");
    let metadata = (0..17)
        .map(|idx| (format!("key_{idx}"), format!("value_{idx}")))
        .collect();

    let error = refinement_for(agent_core::ProviderKind::OpenAi)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::OpenAi(OpenAiOptions {
                metadata,
                ..Default::default()
            })),
        )
        .expect_err("refinement should reject oversized metadata");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert!(error.message.contains("at most 16 pairs"));
}

#[test]
fn openai_refinement_rejects_metadata_key_longer_than_sixty_four_characters() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    let error = refinement_for(agent_core::ProviderKind::OpenAi)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::OpenAi(OpenAiOptions {
                metadata: BTreeMap::from([("k".repeat(65), "trace-1".to_string())]),
                ..Default::default()
            })),
        )
        .expect_err("refinement should reject oversized metadata key");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert!(error.message.contains("keys must be at most 64 characters"));
}

#[test]
fn openai_refinement_rejects_metadata_value_longer_than_five_hundred_twelve_characters() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    let error = refinement_for(agent_core::ProviderKind::OpenAi)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::OpenAi(OpenAiOptions {
                metadata: BTreeMap::from([("trace_id".to_string(), "v".repeat(513))]),
                ..Default::default()
            })),
        )
        .expect_err("refinement should reject oversized metadata value");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert!(
        error
            .message
            .contains("values must be at most 512 characters")
    );
}

#[test]
fn openai_refinement_rejects_safety_identifier_longer_than_sixty_four_characters() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    let error = refinement_for(agent_core::ProviderKind::OpenAi)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::OpenAi(OpenAiOptions {
                safety_identifier: Some("s".repeat(65)),
                ..Default::default()
            })),
        )
        .expect_err("refinement should reject oversized safety identifier");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert!(error.message.contains("safety_identifier"));
}
