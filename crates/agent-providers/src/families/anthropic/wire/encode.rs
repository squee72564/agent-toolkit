use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use agent_core::types::{
    ContentPart, Message, MessageRole, ResponseFormat, RuntimeWarning, TaskRequest, ToolChoice,
    ToolDefinition, ToolResultContent,
};

use super::schema_rules::{canonicalize_json, permissive_json_object_schema, stable_json_string};
use super::{AnthropicEncodedRequest, AnthropicFamilyError};

const DEFAULT_MAX_TOKENS: u32 = 1024;

const WARN_DEFAULT_MAX_TOKENS_APPLIED: &str = "anthropic.encode.default_max_tokens_applied";

#[derive(Debug, Clone, PartialEq)]
struct WireMessage {
    role: &'static str,
    content: Vec<Value>,
}

impl WireMessage {
    fn into_json(self) -> Value {
        json!({
            "role": self.role,
            "content": self.content,
        })
    }
}

pub(crate) fn encode_anthropic_request(
    task: &TaskRequest,
    model_id: &str,
) -> Result<AnthropicEncodedRequest, AnthropicFamilyError> {
    validate_request(task, model_id)?;

    let mut warnings = Vec::new();

    let TaskRequest {
        messages,
        tools,
        tool_choice,
        response_format,
    } = task.clone();

    let (system, non_system_messages) = map_system_prefix(messages)?;
    let mapped_messages = map_non_system_messages(non_system_messages)?;
    let merged_messages = merge_consecutive_messages(mapped_messages);
    validate_tool_ordering(&merged_messages)?;

    if merged_messages.is_empty() {
        return Err(AnthropicFamilyError::validation("empty messages"));
    }

    // Validate tool_choice against the original tool definitions before consuming them.
    let tool_choice = map_tool_choice(&tools, &tool_choice)?;
    let tools = map_tools(tools)?;
    let output_config = map_response_format(response_format, &merged_messages)?;

    let mut body = Map::new();
    body.insert("model".to_string(), Value::String(model_id.to_string()));
    body.insert(
        "max_tokens".to_string(),
        Value::from({
            push_warning(
                &mut warnings,
                WARN_DEFAULT_MAX_TOKENS_APPLIED,
                format!(
                    "max_output_tokens not set; defaulting to {DEFAULT_MAX_TOKENS} for Anthropic"
                ),
            );
            DEFAULT_MAX_TOKENS
        }),
    );
    body.insert(
        "messages".to_string(),
        Value::Array(
            merged_messages
                .into_iter()
                .map(WireMessage::into_json)
                .collect(),
        ),
    );

    if let Some(system_blocks) = system {
        body.insert("system".to_string(), Value::Array(system_blocks));
    }
    if !tools.is_empty() {
        body.insert("tools".to_string(), Value::Array(tools));
    }
    body.insert("tool_choice".to_string(), tool_choice);

    if let Some(output_config) = output_config {
        body.insert("output_config".to_string(), output_config);
    }
    Ok(AnthropicEncodedRequest {
        body: Value::Object(body),
        warnings,
    })
}

fn validate_request(task: &TaskRequest, model_id: &str) -> Result<(), AnthropicFamilyError> {
    if model_id.trim().is_empty() {
        return Err(AnthropicFamilyError::validation("missing model_id"));
    }

    validate_tool_choice(task)?;

    Ok(())
}

fn validate_tool_choice(task: &TaskRequest) -> Result<(), AnthropicFamilyError> {
    if task.tools.is_empty() {
        if matches!(task.tool_choice, ToolChoice::Required) {
            return Err(AnthropicFamilyError::validation(
                "tool_choice 'required' requires at least one tool definition",
            ));
        }

        if matches!(task.tool_choice, ToolChoice::Specific { .. }) {
            return Err(AnthropicFamilyError::validation(
                "tool_choice 'specific' requires at least one tool definition",
            ));
        }
    }

    if let ToolChoice::Specific { name } = &task.tool_choice {
        if name.trim().is_empty() {
            return Err(AnthropicFamilyError::validation(
                "tool_choice specific requires a non-empty tool name",
            ));
        }
        if !task.tools.iter().any(|tool| tool.name == *name) {
            return Err(AnthropicFamilyError::validation(format!(
                "tool_choice specific references unknown tool: {name}",
            )));
        }
    }

    Ok(())
}

fn map_system_prefix(
    messages: Vec<Message>,
) -> Result<(Option<Vec<Value>>, Vec<Message>), AnthropicFamilyError> {
    let mut system_blocks = Vec::new();
    let mut remaining_messages = Vec::new();
    let mut non_system_started = false;

    for message in messages {
        let Message { role, content } = message;
        if role == MessageRole::System && non_system_started {
            return Err(AnthropicFamilyError::validation(
                "system messages must form a contiguous prefix for Anthropic",
            ));
        }

        if role == MessageRole::System {
            for part in content {
                match part {
                    ContentPart::Text { text } => system_blocks.push(json!({
                        "type": "text",
                        "text": text,
                    })),
                    _ => {
                        return Err(AnthropicFamilyError::validation(
                            "system messages only support text content",
                        ));
                    }
                }
            }
            continue;
        }

        non_system_started = true;
        remaining_messages.push(Message { role, content });
    }

    let system = if system_blocks.is_empty() {
        None
    } else {
        Some(system_blocks)
    };

    Ok((system, remaining_messages))
}

fn map_non_system_messages(
    messages: Vec<Message>,
) -> Result<Vec<WireMessage>, AnthropicFamilyError> {
    let mut mapped = Vec::new();
    let mut seen_tool_call_ids = BTreeSet::new();

    for message in messages {
        let Message { role, content } = message;
        let wire_role = match role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "user",
            MessageRole::System => unreachable!(),
        };

        let mut blocks = Vec::new();
        for part in content {
            match part {
                ContentPart::Text { text } => {
                    if role == MessageRole::Tool {
                        return Err(AnthropicFamilyError::validation(
                            "tool messages must contain tool_result content only",
                        ));
                    }

                    blocks.push(json!({
                        "type": "text",
                        "text": text,
                    }));
                }
                ContentPart::ToolCall { tool_call } => {
                    if role != MessageRole::Assistant {
                        return Err(AnthropicFamilyError::validation(
                            "tool_call content is only valid in assistant messages",
                        ));
                    }
                    let tool_call_id = tool_call.id;
                    if tool_call_id.trim().is_empty() {
                        return Err(AnthropicFamilyError::validation(
                            "tool_call content requires a non-empty tool_call id",
                        ));
                    }
                    let tool_call_name = tool_call.name;
                    if tool_call_name.trim().is_empty() {
                        return Err(AnthropicFamilyError::validation(
                            "tool_call content requires a non-empty tool_call name",
                        ));
                    }
                    if !tool_call.arguments_json.is_object() {
                        return Err(AnthropicFamilyError::validation(format!(
                            "tool_call '{}' arguments_json must be a JSON object",
                            tool_call_name
                        )));
                    }

                    if !seen_tool_call_ids.insert(tool_call_id.clone()) {
                        return Err(AnthropicFamilyError::protocol_violation(format!(
                            "duplicate assistant tool_call id '{}'",
                            tool_call_id
                        )));
                    }
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": tool_call_id,
                        "name": tool_call_name,
                        "input": tool_call.arguments_json,
                    }));
                }
                ContentPart::ToolResult { tool_result } => {
                    if role != MessageRole::Tool {
                        return Err(AnthropicFamilyError::validation(
                            "tool_result content is only valid in tool messages",
                        ));
                    }
                    let tool_call_id = tool_result.tool_call_id;
                    if tool_call_id.trim().is_empty() {
                        return Err(AnthropicFamilyError::validation(
                            "tool_result content requires a non-empty tool_call_id",
                        ));
                    }
                    if !seen_tool_call_ids.contains(&tool_call_id) {
                        return Err(AnthropicFamilyError::protocol_violation(format!(
                            "tool_result references unknown tool_call_id: {}",
                            tool_call_id
                        )));
                    }

                    let content = tool_result_content_as_text_blocks(tool_result.content)?;
                    blocks.push(json!({
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content,
                    }));
                }
            }
        }

        if blocks.is_empty() {
            return Err(AnthropicFamilyError::validation(
                "message content must contain at least one encodable part",
            ));
        }

        mapped.push(WireMessage {
            role: wire_role,
            content: blocks,
        });
    }

    Ok(mapped)
}

fn tool_result_content_as_text_blocks(
    content: ToolResultContent,
) -> Result<Vec<Value>, AnthropicFamilyError> {
    match content {
        ToolResultContent::Text { text } => Ok(vec![json!({
            "type": "text",
            "text": text,
        })]),
        ToolResultContent::Json { value } => Ok(vec![json!({
            "type": "text",
            "text": stable_json_string(&canonicalize_json(&value)),
        })]),
        ToolResultContent::Parts { parts } => {
            let mut blocks = Vec::new();
            for part in parts {
                match part {
                    ContentPart::Text { text } => blocks.push(json!({
                        "type": "text",
                        "text": text,
                    })),
                    _ => {
                        return Err(AnthropicFamilyError::validation(
                            "tool_result parts content must contain only text parts",
                        ));
                    }
                }
            }
            Ok(blocks)
        }
    }
}

fn merge_consecutive_messages(messages: Vec<WireMessage>) -> Vec<WireMessage> {
    let mut merged: Vec<WireMessage> = Vec::new();

    for mut message in messages {
        if let Some(last) = merged.last_mut()
            && last.role == message.role
        {
            last.content.append(&mut message.content);
            if last.role == "user" {
                reorder_user_content_tool_results_first(&mut last.content);
            }
            continue;
        }

        if message.role == "user" {
            reorder_user_content_tool_results_first(&mut message.content);
        }
        merged.push(message);
    }

    merged
}

fn reorder_user_content_tool_results_first(content: &mut Vec<Value>) {
    let mut tool_results = Vec::new();
    let mut other_blocks = Vec::new();

    for block in content.drain(..) {
        let is_tool_result = block
            .as_object()
            .and_then(|obj| obj.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "tool_result");

        if is_tool_result {
            tool_results.push(block);
        } else {
            other_blocks.push(block);
        }
    }

    content.extend(tool_results);
    content.extend(other_blocks);
}

fn validate_tool_ordering(messages: &[WireMessage]) -> Result<(), AnthropicFamilyError> {
    for (index, message) in messages.iter().enumerate() {
        if message.role != "assistant" {
            continue;
        }

        let pending_tool_ids = message
            .content
            .iter()
            .filter_map(|block| {
                let block_obj = block.as_object()?;
                if block_obj.get("type")?.as_str()? != "tool_use" {
                    return None;
                }
                block_obj.get("id")?.as_str().map(str::to_string)
            })
            .collect::<Vec<_>>();

        if pending_tool_ids.is_empty() {
            continue;
        }

        let Some(next_message) = messages.get(index + 1) else {
            return Err(AnthropicFamilyError::protocol_violation(
                "assistant tool_use requires a following user tool_result message",
            ));
        };

        if next_message.role != "user" {
            return Err(AnthropicFamilyError::protocol_violation(
                "assistant tool_use must be followed by a user message containing tool_result blocks",
            ));
        }

        let mut prefix_tool_result_ids = Vec::new();
        for block in &next_message.content {
            let Some(block_obj) = block.as_object() else {
                return Err(AnthropicFamilyError::protocol_violation(
                    "anthropic user content block must be object",
                ));
            };
            let Some(block_type) = block_obj.get("type").and_then(Value::as_str) else {
                return Err(AnthropicFamilyError::protocol_violation(
                    "anthropic user content block missing type",
                ));
            };

            if block_type != "tool_result" {
                break;
            }
            let Some(tool_use_id) = block_obj.get("tool_use_id").and_then(Value::as_str) else {
                return Err(AnthropicFamilyError::protocol_violation(
                    "tool_result block missing tool_use_id",
                ));
            };
            prefix_tool_result_ids.push(tool_use_id.to_string());
        }

        if prefix_tool_result_ids.is_empty() {
            return Err(AnthropicFamilyError::protocol_violation(
                "assistant tool_use requires tool_result blocks at the start of the next user message",
            ));
        }

        for pending_id in pending_tool_ids {
            if !prefix_tool_result_ids.iter().any(|id| id == &pending_id) {
                return Err(AnthropicFamilyError::protocol_violation(format!(
                    "missing tool_result for assistant tool_use id '{pending_id}' in following user message",
                )));
            }
        }
    }

    Ok(())
}

fn map_tools(tools: Vec<ToolDefinition>) -> Result<Vec<Value>, AnthropicFamilyError> {
    tools
        .into_iter()
        .map(map_tool_definition)
        .collect::<Result<Vec<_>, _>>()
}

fn map_tool_definition(tool: ToolDefinition) -> Result<Value, AnthropicFamilyError> {
    let ToolDefinition {
        name,
        description,
        parameters_schema,
    } = tool;

    if name.trim().is_empty() {
        return Err(AnthropicFamilyError::validation(
            "tool definitions require non-empty names",
        ));
    }
    if name.chars().count() > 128 {
        return Err(AnthropicFamilyError::validation(format!(
            "tool '{}' name exceeds 128 characters",
            name
        )));
    }
    if !parameters_schema.is_object() {
        return Err(AnthropicFamilyError::validation(format!(
            "tool '{}' parameters_schema must be a JSON object",
            name
        )));
    }

    let mut mapped = Map::new();
    mapped.insert("name".to_string(), Value::String(name));
    if let Some(description) = description {
        mapped.insert("description".to_string(), Value::String(description));
    }
    mapped.insert("input_schema".to_string(), parameters_schema);

    Ok(Value::Object(mapped))
}

fn map_tool_choice(
    tools: &[ToolDefinition],
    tool_choice: &ToolChoice,
) -> Result<Value, AnthropicFamilyError> {
    match tool_choice {
        ToolChoice::None => Ok(json!({ "type": "none" })),
        ToolChoice::Auto => Ok(json!({ "type": "auto" })),
        ToolChoice::Required => Ok(json!({ "type": "any" })),
        ToolChoice::Specific { name } => {
            if name.trim().is_empty() {
                return Err(AnthropicFamilyError::validation(
                    "tool_choice specific requires a non-empty tool name",
                ));
            }
            if !tools.iter().any(|tool| tool.name == *name) {
                return Err(AnthropicFamilyError::validation(format!(
                    "tool_choice specific references unknown tool: {name}",
                )));
            }
            Ok(json!({
                "type": "tool",
                "name": name,
                "disable_parallel_tool_use": true,
            }))
        }
    }
}

fn map_response_format(
    response_format: ResponseFormat,
    messages: &[WireMessage],
) -> Result<Option<Value>, AnthropicFamilyError> {
    match response_format {
        ResponseFormat::Text => Ok(None),
        ResponseFormat::JsonObject => {
            validate_no_assistant_prefill(messages)?;
            Ok(Some(json!({
                "format": {
                    "type": "json_schema",
                    "schema": permissive_json_object_schema(),
                }
            })))
        }
        ResponseFormat::JsonSchema { schema, .. } => {
            validate_no_assistant_prefill(messages)?;
            Ok(Some(json!({
                "format": {
                    "type": "json_schema",
                    "schema": schema,
                }
            })))
        }
    }
}

fn validate_no_assistant_prefill(messages: &[WireMessage]) -> Result<(), AnthropicFamilyError> {
    if messages
        .last()
        .is_some_and(|message| message.role == "assistant")
    {
        return Err(AnthropicFamilyError::validation(
            "json response formats are incompatible with assistant-prefill final messages",
        ));
    }
    Ok(())
}

fn push_warning(warnings: &mut Vec<RuntimeWarning>, code: &str, message: impl Into<String>) {
    warnings.push(RuntimeWarning {
        code: code.to_string(),
        message: message.into(),
    });
}
