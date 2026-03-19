use std::collections::BTreeMap;

use agent_core::types::ProviderKind;
use agent_core::types::{
    ContentPart, Message, MessageRole, ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
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

fn plan_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
) -> Result<crate::request_plan::ProviderRequestPlan, crate::error::AdapterError> {
    let mut encoded = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible).encode_task(
        task,
        model,
        response_mode,
        None,
    )?;
    refinement_for(ProviderKind::OpenAi).refine_request(task, model, &mut encoded, None)?;
    Ok(encoded.into())
}

#[test]
fn openai_request_error_maps_into_adapter_error() {
    let adapter_error = plan_request(&base_task(), "", ResponseMode::NonStreaming)
        .expect_err("planning should fail");

    assert_eq!(adapter_error.provider, ProviderKind::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::PlanRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert!(!adapter_error.message.is_empty());
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn openai_request_error_preserves_source_chain() {
    let adapter_error = plan_request(&base_task(), "", ResponseMode::NonStreaming)
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
    let encoded = plan_request(&base_task(), MODEL_ID, ResponseMode::NonStreaming)
        .expect("planning should succeed");

    assert_eq!(encoded.body["model"], MODEL_ID);
    assert!(encoded.body["input"].is_array());
}
