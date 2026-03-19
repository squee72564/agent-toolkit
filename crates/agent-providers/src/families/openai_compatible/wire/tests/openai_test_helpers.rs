use std::collections::BTreeMap;

use agent_core::types::{Message, ResponseFormat, TaskRequest, ToolChoice};

use crate::families::openai_compatible::wire::{
    OpenAiEncodedRequest, OpenAiFamilyError, encode::encode_openai_request as encode_task_request,
};

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

pub fn base_request(messages: Vec<Message>) -> TaskRequest {
    TaskRequest {
        messages,
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
