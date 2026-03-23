use agent_core::types::{
    AnthropicFamilyOptions, FamilyOptions, Message, ResponseFormat, ResponseMode, TaskRequest,
    ToolChoice,
};

use crate::families::anthropic::wire::{
    AnthropicEncodedRequest, AnthropicFamilyError,
    encode::encode_anthropic_request as encode_task_request,
};
use crate::interfaces::codec_for;
use crate::request_plan::EncodedFamilyRequest;

pub const MODEL_ID: &str = "claude-sonnet-4.6";

pub fn encode_anthropic_request(
    task: TaskRequest,
) -> Result<AnthropicEncodedRequest, AnthropicFamilyError> {
    encode_task_request(&task, MODEL_ID)
}

pub fn plan_anthropic_family_request(
    task: &TaskRequest,
    response_mode: ResponseMode,
    family_options: Option<AnthropicFamilyOptions>,
) -> Result<EncodedFamilyRequest, crate::error::AdapterError> {
    plan_anthropic_family_request_with_model(task, MODEL_ID, response_mode, family_options)
}

pub fn plan_anthropic_family_request_with_model(
    task: &TaskRequest,
    model_id: &str,
    response_mode: ResponseMode,
    family_options: Option<AnthropicFamilyOptions>,
) -> Result<EncodedFamilyRequest, crate::error::AdapterError> {
    let family_options = family_options
        .as_ref()
        .map(|options| FamilyOptions::Anthropic(options.clone()));

    codec_for(agent_core::ProviderFamilyId::Anthropic).encode_task(
        task,
        model_id,
        response_mode,
        family_options.as_ref(),
    )
}

pub fn base_request(messages: Vec<Message>) -> TaskRequest {
    TaskRequest {
        messages,
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
    }
}
