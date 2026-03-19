use std::collections::BTreeMap;

use agent_core::types::{Message, ResponseFormat, TaskRequest, ToolChoice};

use crate::families::anthropic::wire::{
    AnthropicEncodedRequest, AnthropicFamilyError,
    encode::encode_anthropic_request as encode_task_request,
};

pub const MODEL_ID: &str = "claude-sonnet-4.6";

pub fn encode_anthropic_request(
    task: TaskRequest,
) -> Result<AnthropicEncodedRequest, AnthropicFamilyError> {
    encode_task_request(&task, MODEL_ID)
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
