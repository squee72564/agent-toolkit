use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value, json};

use agent_core::types::{
    ContentPart, Message, MessageRole, Request, ResponseFormat, RuntimeWarning, ToolChoice,
    ToolDefinition, ToolResult, ToolResultContent,
};

use super::schema_rules::{canonicalize_json, is_strict_compatible_schema, stable_json_string};
use super::{OpenAiEncodedRequest, OpenAiSpecError};

pub(crate) struct OpenAiEncodeInput<'a> {
    pub(crate) model_id: &'a str,
    pub(crate) messages: Vec<Message>,
    pub(crate) tools: Vec<ToolDefinition>,
    pub(crate) tool_choice: ToolChoice,
    pub(crate) response_format: ResponseFormat,
    pub(crate) temperature: Option<f32>,
    pub(crate) top_p: Option<f32>,
    pub(crate) max_output_tokens: Option<u32>,
    pub(crate) stop: &'a [String],
    pub(crate) metadata: BTreeMap<String, String>,
}

pub(crate) fn encode_openai_request(req: Request) -> Result<OpenAiEncodedRequest, OpenAiSpecError> {
    let Request {
        model_id,
        stream: _,
        messages,
        tools,
        tool_choice,
        response_format,
        metadata,
        temperature,
        top_p,
        max_output_tokens,
        stop,
    } = req;

    encode_openai_request_parts(OpenAiEncodeInput {
        model_id: &model_id,
        messages,
        tools,
        tool_choice,
        response_format,
        temperature,
        top_p,
        max_output_tokens,
        stop: &stop,
        metadata,
    })
}

pub(crate) fn encode_openai_request_parts(
    input: OpenAiEncodeInput<'_>,
) -> Result<OpenAiEncodedRequest, OpenAiSpecError> {
    let OpenAiEncodeInput {
        model_id,
        messages,
        tools,
        tool_choice,
        response_format,
        temperature,
        top_p,
        max_output_tokens,
        stop,
        metadata,
    } = input;

    if model_id.trim().is_empty() {
        return Err(OpenAiSpecError::validation("model_id must not be empty"));
    }

    let mut warnings = Vec::new();
    let text_format = map_response_format(response_format)?;
    // Validate tool_choice against the original tool definitions before consuming them.
    let tool_choice = map_tool_choice(&tools, &tool_choice)?;
    let tools = map_tools(tools, &mut warnings)?;
    let input = map_messages(messages)?;

    if input.is_empty() {
        return Err(OpenAiSpecError::validation("empty input"));
    }

    let mut body = Map::new();
    body.insert("model".to_string(), Value::String(model_id.to_string()));
    body.insert("store".to_string(), Value::Bool(false));
    body.insert("input".to_string(), Value::Array(input));
    body.insert("text".to_string(), json!({ "format": text_format }));

    if !tools.is_empty() {
        body.insert("tools".to_string(), Value::Array(tools));
    }

    body.insert("tool_choice".to_string(), tool_choice);

    if let Some(temperature) = temperature {
        body.insert("temperature".to_string(), json!(temperature));
    }

    if let Some(max_output_tokens) = max_output_tokens {
        body.insert("max_output_tokens".to_string(), json!(max_output_tokens));
    }

    if !metadata.is_empty() {
        body.insert("metadata".to_string(), json!(metadata));
    }

    if top_p.is_some() {
        push_warning(
            &mut warnings,
            "openai.encode.ignored_top_p",
            "top_p is currently not mapped for OpenAI Responses API and was ignored",
        );
    }

    if !stop.is_empty() {
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

fn map_response_format(response_format: ResponseFormat) -> Result<Value, OpenAiSpecError> {
    match response_format {
        ResponseFormat::Text => Ok(json!({ "type": "text" })),
        ResponseFormat::JsonObject => Ok(json!({ "type": "json_object" })),
        ResponseFormat::JsonSchema { name, schema } => {
            if name.trim().is_empty() {
                return Err(OpenAiSpecError::validation(
                    "json_schema response format requires a non-empty name",
                ));
            }

            if !schema.is_object() {
                return Err(OpenAiSpecError::validation(
                    "json_schema response format requires schema to be a JSON object",
                ));
            }

            Ok(json!({
                "type": "json_schema",
                "name": name,
                "schema": schema,
                "strict": true,
            }))
        }
    }
}

fn map_tool_choice(
    tools: &[ToolDefinition],
    tool_choice: &ToolChoice,
) -> Result<Value, OpenAiSpecError> {
    if tools.is_empty() {
        if matches!(tool_choice, ToolChoice::Required) {
            return Err(OpenAiSpecError::validation(
                "tool_choice 'required' requires at least one tool definition",
            ));
        }

        if let ToolChoice::Specific { .. } = tool_choice {
            return Err(OpenAiSpecError::validation(
                "tool_choice 'specific' requires at least one tool definition",
            ));
        }
    }

    match tool_choice {
        ToolChoice::None => Ok(Value::String("none".to_string())),
        ToolChoice::Auto => Ok(Value::String("auto".to_string())),
        ToolChoice::Required => Ok(Value::String("required".to_string())),
        ToolChoice::Specific { name } => {
            if name.trim().is_empty() {
                return Err(OpenAiSpecError::validation(
                    "tool_choice specific requires a non-empty tool name",
                ));
            }

            let found = tools.iter().any(|tool| tool.name == *name);
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
    tools: Vec<ToolDefinition>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<Vec<Value>, OpenAiSpecError> {
    let mut seen_tool_names = HashSet::new();
    let mut mapped_tools = Vec::new();

    for tool in tools {
        if !seen_tool_names.insert(tool.name.clone()) {
            return Err(OpenAiSpecError::validation(format!(
                "duplicate tool definition name: {}",
                tool.name
            )));
        }
        mapped_tools.push(map_tool_definition(tool, warnings)?);
    }

    Ok(mapped_tools)
}

fn map_tool_definition(
    tool: ToolDefinition,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<Value, OpenAiSpecError> {
    let ToolDefinition {
        name,
        description,
        parameters_schema,
    } = tool;

    if name.trim().is_empty() {
        return Err(OpenAiSpecError::validation(
            "tool definition requires non-empty name",
        ));
    }

    if !parameters_schema.is_object() {
        return Err(OpenAiSpecError::validation(format!(
            "tool '{}' parameters_schema must be a JSON object",
            name
        )));
    }

    let strict = is_strict_compatible_schema(&parameters_schema);
    if !strict {
        push_warning(
            warnings,
            "openai.encode.non_strict_tool_schema",
            format!(
                "tool '{}' schema is not strict-compatible; emitted strict=false",
                name
            ),
        );
    }

    let mut payload = Map::new();
    payload.insert("type".to_string(), Value::String("function".to_string()));
    payload.insert("name".to_string(), Value::String(name));

    if let Some(description) = description {
        payload.insert("description".to_string(), Value::String(description));
    }

    payload.insert("parameters".to_string(), parameters_schema);
    payload.insert("strict".to_string(), Value::Bool(strict));

    Ok(Value::Object(payload))
}

fn map_messages(messages: Vec<Message>) -> Result<Vec<Value>, OpenAiSpecError> {
    let mut input_items = Vec::new();
    let mut seen_tool_call_ids: HashSet<String> = HashSet::new();

    for message in messages {
        let Message { role, content } = message;
        let mut message_parts = Vec::new();

        for part in content {
            match part {
                ContentPart::Text { text } => {
                    if role == MessageRole::Tool {
                        return Err(OpenAiSpecError::validation(
                            "tool role messages cannot contain plain text content",
                        ));
                    }

                    let part_type = if role == MessageRole::Assistant {
                        "output_text"
                    } else {
                        "input_text"
                    };

                    message_parts.push(json!({ "type": part_type, "text": text }));
                }
                ContentPart::ToolCall { tool_call } => {
                    if role != MessageRole::Assistant {
                        return Err(OpenAiSpecError::validation(
                            "tool_call content is only valid for assistant role messages",
                        ));
                    }
                    let tool_id = tool_call.id;
                    if tool_id.trim().is_empty() {
                        return Err(OpenAiSpecError::validation(
                            "assistant tool_call id must not be empty",
                        ));
                    }
                    let tool_name = tool_call.name;
                    if tool_name.trim().is_empty() {
                        return Err(OpenAiSpecError::validation(
                            "assistant tool_call name must not be empty",
                        ));
                    }
                    if !seen_tool_call_ids.insert(tool_id.clone()) {
                        return Err(OpenAiSpecError::protocol_violation(format!(
                            "duplicate_tool_call_id: {}",
                            tool_id
                        )));
                    }

                    flush_message_item(&mut input_items, &role, &mut message_parts);

                    let arguments =
                        serde_json::to_string(&tool_call.arguments_json).map_err(|e| {
                            OpenAiSpecError::encode_with_source(
                                format!(
                                    "failed to serialize tool_call arguments for '{}'",
                                    tool_name
                                ),
                                e,
                            )
                        })?;

                    input_items.push(json!({
                        "type": "function_call",
                        "call_id": tool_id,
                        "name": tool_name,
                        "arguments": arguments
                    }));
                }
                ContentPart::ToolResult { tool_result } => {
                    if role != MessageRole::Tool {
                        return Err(OpenAiSpecError::validation(
                            "tool_result content is only valid for tool role messages",
                        ));
                    }
                    let ToolResult {
                        tool_call_id,
                        content,
                        raw_provider_content,
                    } = tool_result;
                    if tool_call_id.trim().is_empty() {
                        return Err(OpenAiSpecError::validation(
                            "tool_result tool_call_id must not be empty",
                        ));
                    }

                    flush_message_item(&mut input_items, &role, &mut message_parts);

                    if !seen_tool_call_ids.contains(&tool_call_id) {
                        return Err(OpenAiSpecError::protocol_violation(format!(
                            "tool_result_without_matching_tool_call: {}",
                            tool_call_id
                        )));
                    }

                    let output =
                        serialize_tool_result_output(content, raw_provider_content.as_ref())?;
                    input_items.push(json!({
                        "type": "function_call_output",
                        "call_id": tool_call_id,
                        "output": output
                    }));
                }
            }
        }

        flush_message_item(&mut input_items, &role, &mut message_parts);
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

fn serialize_tool_result_output(
    content: ToolResultContent,
    raw_provider_content: Option<&Value>,
) -> Result<String, OpenAiSpecError> {
    if let Some(raw_provider_content) = raw_provider_content
        && let Some(raw_text) = raw_provider_content.as_str()
    {
        return Ok(raw_text.to_string());
    }

    match content {
        ToolResultContent::Text { text } => Ok(text),
        ToolResultContent::Json { value } => Ok(stable_json_string(&canonicalize_json(&value))),
        ToolResultContent::Parts { parts } => {
            let mut lines = Vec::new();

            for part in parts {
                match part {
                    ContentPart::Text { text } => lines.push(text),
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
