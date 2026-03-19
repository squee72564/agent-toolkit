use crate::error::{AdapterErrorKind, AdapterOperation};
use crate::interfaces::codec_for;
use crate::interfaces::refinement_for;
use agent_core::{
    ContentPart, Message, MessageRole, ProviderKind, ResponseFormat, ResponseMode, TaskRequest,
    ToolChoice,
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
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: std::collections::BTreeMap::new(),
    }
}

fn plan_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
) -> Result<crate::request_plan::ProviderRequestPlan, crate::error::AdapterError> {
    let mut encoded = codec_for(agent_core::ProviderFamilyId::Anthropic).encode_task(
        task,
        model,
        response_mode,
        None,
    )?;
    refinement_for(ProviderKind::Anthropic).refine_request(task, model, &mut encoded, None)?;
    Ok(encoded.into())
}

#[test]
fn anthropic_request_error_maps_into_adapter_error() {
    let adapter_error = plan_request(&base_task(), "", ResponseMode::NonStreaming)
        .expect_err("planning should fail");

    assert_eq!(adapter_error.provider, ProviderKind::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::PlanRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert!(!adapter_error.message.is_empty());
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn anthropic_request_error_preserves_source_chain() {
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
