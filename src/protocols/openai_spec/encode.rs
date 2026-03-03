use serde_json::{Map, Value, json};

use crate::core::types::{
    ContentPart, MessageRole, Request, ResponseFormat, RuntimeWarning, ToolChoice, ToolDefinition,
    ToolResult, ToolResultContent,
};

use super::schema_rules::{canonicalize_json, is_strict_compatible_schema, stable_json_string};
use super::{OpenAiEncodedRequest, OpenAiSpecError};

pub(crate) fn encode_openai_request(
    req: &Request,
) -> Result<OpenAiEncodedRequest, OpenAiSpecError> {
    let mut warnings = Vec::new();
    let text_format = map_response_format(req)?;
    let tool_choice = map_tool_choice(req)?;
    let tools = map_tools(req, &mut warnings)?;
    let input = map_messages(req)?;

    if input.is_empty() {
        return Err(OpenAiSpecError::validation("empty input"));
    }

    let mut body = Map::new();
    body.insert("model".to_string(), Value::String(req.model_id.clone()));
    body.insert("store".to_string(), Value::Bool(false));
    body.insert("input".to_string(), Value::Array(input));
    body.insert("text".to_string(), json!({ "format": text_format }));

    if !tools.is_empty() {
        body.insert("tools".to_string(), Value::Array(tools));
    }

    body.insert("tool_choice".to_string(), tool_choice);

    if let Some(temperature) = req.temperature {
        body.insert("temperature".to_string(), json!(temperature));
    }

    if let Some(max_output_tokens) = req.max_output_tokens {
        body.insert("max_output_tokens".to_string(), json!(max_output_tokens));
    }

    if !req.metadata.is_empty() {
        body.insert("metadata".to_string(), json!(req.metadata));
    }

    if req.top_p.is_some() {
        push_warning(
            &mut warnings,
            "openai.encode.ignored_top_p",
            "top_p is currently not mapped for OpenAI Responses API and was ignored",
        );
    }

    if !req.stop.is_empty() {
        push_warning(
            &mut warnings,
            "openai.encode.ignored_stop",
            "stop sequences are currently not mapped for OpenAI Responses API and were ignored",
        );
    }

    Ok(OpenAiEncodedRequest {
        body: Value::Object(body),
        warnings,
    })
}

fn map_response_format(req: &Request) -> Result<Value, OpenAiSpecError> {
    match &req.response_format {
        ResponseFormat::Text => Ok(json!({ "type": "text" })),
        ResponseFormat::JsonObject => Ok(json!({ "type": "json_object" })),
        ResponseFormat::JsonSchema { name, schema } => Ok(json!({
            "type": "json_schema",
            "name": name,
            "schema": schema,
            "strict": true,
        })),
    }
}

fn map_tool_choice(req: &Request) -> Result<Value, OpenAiSpecError> {
    if req.tools.is_empty() {
        if matches!(req.tool_choice, ToolChoice::Required) {
            return Err(OpenAiSpecError::validation(
                "tool_choice 'required' requires at least one tool definition",
            ));
        }

        if let ToolChoice::Specific { .. } = &req.tool_choice {
            return Err(OpenAiSpecError::validation(
                "tool_choice 'specific' requires at least one tool definition",
            ));
        }
    }

    match &req.tool_choice {
        ToolChoice::None => Ok(Value::String("none".to_string())),
        ToolChoice::Auto => Ok(Value::String("auto".to_string())),
        ToolChoice::Required => Ok(Value::String("required".to_string())),
        ToolChoice::Specific { name } => {
            if name.trim().is_empty() {
                return Err(OpenAiSpecError::validation(
                    "tool_choice specific requires a non-empty tool name",
                ));
            }

            let found = req.tools.iter().any(|tool| tool.name == *name);
            if !found {
                return Err(OpenAiSpecError::validation(format!(
                    "tool_choice specific references unknown tool: {name}"
                )));
            }

            Ok(json!({ "type": "function", "name": name }))
        }
    }
}

fn map_tools(
    req: &Request,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<Vec<Value>, OpenAiSpecError> {
    let mut tools = Vec::new();

    for tool in &req.tools {
        tools.push(map_tool_definition(tool, warnings)?);
    }

    Ok(tools)
}

fn map_tool_definition(
    tool: &ToolDefinition,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<Value, OpenAiSpecError> {
    if tool.name.trim().is_empty() {
        return Err(OpenAiSpecError::validation(
            "tool definition requires non-empty name",
        ));
    }

    if !tool.parameters_schema.is_object() {
        return Err(OpenAiSpecError::validation(format!(
            "tool '{}' parameters_schema must be a JSON object",
            tool.name
        )));
    }

    let strict = is_strict_compatible_schema(&tool.parameters_schema);
    if !strict {
        push_warning(
            warnings,
            "openai.encode.non_strict_tool_schema",
            format!(
                "tool '{}' schema is not strict-compatible; emitted strict=false",
                tool.name
            ),
        );
    }

    let mut payload = Map::new();
    payload.insert("type".to_string(), Value::String("function".to_string()));
    payload.insert("name".to_string(), Value::String(tool.name.clone()));

    if let Some(description) = &tool.description {
        payload.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }

    payload.insert("parameters".to_string(), tool.parameters_schema.clone());
    payload.insert("strict".to_string(), Value::Bool(strict));

    Ok(Value::Object(payload))
}

fn map_messages(req: &Request) -> Result<Vec<Value>, OpenAiSpecError> {
    let mut input_items = Vec::new();
    let mut seen_tool_call_ids: Vec<String> = Vec::new();

    for message in &req.messages {
        let mut message_parts = Vec::new();

        for part in &message.content {
            match part {
                ContentPart::Text { text } => {
                    if message.role == MessageRole::Tool {
                        return Err(OpenAiSpecError::validation(
                            "tool role messages cannot contain plain text content",
                        ));
                    }

                    let part_type = if message.role == MessageRole::Assistant {
                        "output_text"
                    } else {
                        "input_text"
                    };

                    message_parts.push(json!({ "type": part_type, "text": text }));
                }
                ContentPart::ToolCall { tool_call } => {
                    if message.role != MessageRole::Assistant {
                        return Err(OpenAiSpecError::validation(
                            "tool_call content is only valid for assistant role messages",
                        ));
                    }

                    flush_message_item(&mut input_items, &message.role, &mut message_parts);

                    let arguments =
                        serde_json::to_string(&tool_call.arguments_json).map_err(|e| {
                            OpenAiSpecError::encode_with_source(
                                format!(
                                    "failed to serialize tool_call arguments for '{}'",
                                    tool_call.name
                                ),
                                e,
                            )
                        })?;

                    seen_tool_call_ids.push(tool_call.id.clone());
                    input_items.push(json!({
                        "type": "function_call",
                        "call_id": tool_call.id,
                        "name": tool_call.name,
                        "arguments": arguments
                    }));
                }
                ContentPart::ToolResult { tool_result } => {
                    if message.role != MessageRole::Tool {
                        return Err(OpenAiSpecError::validation(
                            "tool_result content is only valid for tool role messages",
                        ));
                    }

                    flush_message_item(&mut input_items, &message.role, &mut message_parts);

                    if !seen_tool_call_ids.contains(&tool_result.tool_call_id) {
                        return Err(OpenAiSpecError::protocol_violation(format!(
                            "tool_result_without_matching_tool_call: {}",
                            tool_result.tool_call_id
                        )));
                    }

                    let output = serialize_tool_result_output(tool_result)?;
                    input_items.push(json!({
                        "type": "function_call_output",
                        "call_id": tool_result.tool_call_id,
                        "output": output
                    }));
                }
            }
        }

        flush_message_item(&mut input_items, &message.role, &mut message_parts);
    }

    Ok(input_items)
}

fn flush_message_item(
    input_items: &mut Vec<Value>,
    role: &MessageRole,
    message_parts: &mut Vec<Value>,
) {
    if message_parts.is_empty() {
        return;
    }

    let role_value = match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => return,
    };

    let content = std::mem::take(message_parts);
    input_items.push(json!({
        "type": "message",
        "role": role_value,
        "content": content
    }));
}

fn serialize_tool_result_output(tool_result: &ToolResult) -> Result<String, OpenAiSpecError> {
    if let Some(raw_provider_content) = &tool_result.raw_provider_content {
        if let Some(raw_text) = raw_provider_content.as_str() {
            return Ok(raw_text.to_string());
        }
    }

    match &tool_result.content {
        ToolResultContent::Text { text } => Ok(text.clone()),
        ToolResultContent::Json { value } => Ok(stable_json_string(&canonicalize_json(value))),
        ToolResultContent::Parts { parts } => {
            let mut lines = Vec::new();

            for part in parts {
                match part {
                    ContentPart::Text { text } => lines.push(text.clone()),
                    _ => {
                        return Err(OpenAiSpecError::validation(
                            "tool_result parts content for OpenAI must contain only text parts",
                        ));
                    }
                }
            }

            Ok(lines.join("\n"))
        }
    }
}

fn push_warning(warnings: &mut Vec<RuntimeWarning>, code: &str, message: impl Into<String>) {
    warnings.push(RuntimeWarning {
        code: code.to_string(),
        message: message.into(),
    });
}
