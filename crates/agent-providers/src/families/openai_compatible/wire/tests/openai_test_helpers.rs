use agent_core::types::{
    FamilyOptions, Message, OpenAiCompatibleOptions, ResponseFormat, ResponseMode, TaskRequest,
    ToolChoice,
};

use crate::families::openai_compatible::wire::{
    OpenAiEncodedRequest, OpenAiFamilyError, encode::encode_openai_request as encode_task_request,
};
use crate::interfaces::codec_for;
use crate::request_plan::EncodedFamilyRequest;

pub const MODEL_ID: &str = "gpt-4.1-mini";

pub fn encode_openai_request(task: TaskRequest) -> Result<OpenAiEncodedRequest, OpenAiFamilyError> {
    encode_task_request(&task, MODEL_ID)
}

pub fn encode_openai_request_with_model(
    task: TaskRequest,
    model_id: &str,
) -> Result<OpenAiEncodedRequest, OpenAiFamilyError> {
    encode_task_request(&task, model_id)
}

pub fn plan_openai_family_request(
    task: &TaskRequest,
    response_mode: ResponseMode,
    family_options: Option<OpenAiCompatibleOptions>,
) -> Result<EncodedFamilyRequest, crate::error::AdapterError> {
    plan_openai_family_request_with_model(task, MODEL_ID, response_mode, family_options)
}

pub fn plan_openai_family_request_with_model(
    task: &TaskRequest,
    model_id: &str,
    response_mode: ResponseMode,
    family_options: Option<OpenAiCompatibleOptions>,
) -> Result<EncodedFamilyRequest, crate::error::AdapterError> {
    let family_options = family_options
        .as_ref()
        .map(|options| FamilyOptions::OpenAiCompatible(options.clone()));

    codec_for(agent_core::ProviderFamilyId::OpenAiCompatible).encode_task(
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
