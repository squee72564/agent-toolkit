use core::fmt;

use serde_json::{Map, Value, json};

use crate::core::types::{
    Request, Response, ResponseFormat
};

use crate::protocols::translator_contract::ProtocolTranslator;
use crate::{ContentPart, MessageRole, ToolDefinition, ToolResult, ToolResultContent};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenAiEncodedRequest {
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenAiDecodeEnvelope {
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAiErrorEnvelope {
    pub message: String,
    pub code: Option<String>,
    pub error_type: Option<String>,
    pub param: Option<String>,
}

#[derive(Debug)]
pub struct OpenAITranslateError {
    message: String
}

impl fmt::Display for OpenAITranslateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = self.message.clone();
        write!(f, "{message}")
    }
}

impl std::error::Error for OpenAITranslateError {}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OpenAiTranslator;

impl ProtocolTranslator for OpenAiTranslator {
    type RequestPayload = OpenAiEncodedRequest;
    type ResponsePayload = OpenAiDecodeEnvelope;

    fn encode_request(
        &self,
        req: &Request
    ) -> Result<Self::RequestPayload, Box<dyn std::error::Error>>
    {
        todo!("We need to determine the actual error shape we are using across the trait");
        match encode_openai_request(req) {
            Ok(req) => Ok(req),
            Err(err) => Err(Box::new(err)),
        }
    }

    fn decode_request(
        &self,
        payload: &Self::ResponsePayload
    ) -> Result<Response, Box<dyn std::error::Error>>
    {
        todo!("DO THIS")
        //decode_openai_request(payload)
    }
}

pub(crate) fn encode_openai_request(
    req: &Request,
) -> Result<OpenAiEncodedRequest, OpenAITranslateError> {

    let text_format = map_response_format(req)?;
    let tools_choice = map_tool_choice(req)?;
    let tools = map_tools(req)?;
    let input = map_messages(req)?;

    if input.is_empty() {
        return Err(OpenAITranslateError { message: "empty input".to_string() })
    }

    let mut body = Map::new();

    body.insert(
        "model".to_string(),
        Value::String(req.model_id.clone()),
    );

    body.insert("store".to_string(), Value::Bool(false));
    body.insert("input".to_string(), Value::Array(input));
    body.insert("text".to_string(), json!({ "format": text_format }));

    if !tools.is_empty() {
        body.insert("tools".to_string(), Value::Array(tools));
    }

    body.insert("tool_choice".to_string(), tools_choice);

    if let Some(temperature) = req.temperature {
        body.insert("temperature".to_string(), json!(temperature));
    }

    if let Some(max_output_tokens) = req.max_output_tokens {
        body.insert("max_output_tokens".to_string(), json!(max_output_tokens));
    }

    if !req.metadata.is_empty() {
        body.insert("metadata".to_string(), json!(req.metadata));
    }

    Ok(OpenAiEncodedRequest {
        body: Value::Object(body),
    })
}

fn map_response_format(
    req: &Request
) -> Result<Value, OpenAITranslateError> {
    match &req.response_format {
        ResponseFormat::Text => Ok(json!({ "type": "text"})),
        ResponseFormat::JsonObject => {
            
            Ok(json!({"type": "json_object"})) 
        },
        ResponseFormat::JsonSchema { name, schema } => {
            Ok(json!({
                "type": "json_schema",
                "name": name,
                "schema": schema,
                "strict": true,
            }))
        },
    }
}

fn map_tool_choice(
    req: &Request
) -> Result<Value, OpenAITranslateError> {
    match &req.tool_choice {
        crate::ToolChoice::None => Ok(Value::String("none".to_string())),
        crate::ToolChoice::Auto => Ok(Value::String("auto".to_string())),
        crate::ToolChoice::Required => Ok(Value::String("required".to_string())),
        crate::ToolChoice::Specific { name } => {
            if name.trim().is_empty() {
                return Err(OpenAITranslateError{
                    message: "tool_choice specific requires a non-empty tool name".to_string(),
                });
            }

            let found = req.tools.iter().any(|tool| tool.name == *name);
            if !found {
                return Err(OpenAITranslateError{
                    message: format!("tool_choice specific references unknown tool: {name}").to_string(),
                });
            }

            Ok(json!({"type": "function", "name": name}))
        }
    }
}

fn map_tools(
    req: &Request
) -> Result<Vec<Value>, OpenAITranslateError> {
    let mut tools = Vec::new();

    for tool in &req.tools {
        tools.push(
            map_tool_definition(tool)?
        );
    }

    Ok(tools)
}

fn map_tool_definition(
    tool: &ToolDefinition,
) -> Result<Value, OpenAITranslateError> {
    if tool.name.trim().is_empty() {
        return Err(OpenAITranslateError{
            message: "tool definition requires non-empty name".to_string(),
        })
    }

    if !tool.parameters_schema.is_object() {
        return Err(OpenAITranslateError{
            message: format!("tool '{}' parameters_schema must be a JSON object", tool.name).to_string(),
        })
    }

    let strict = is_strict_compatible_schema(&tool.parameters_schema);

    if !strict {
        // Warning that strict is disabled as tool schema is not compatible
    }

    let mut payload = Map::new();

    payload.insert("type".to_string(), Value::String("function".to_string()));
    payload.insert("name".to_string(), Value::String(tool.name.clone()));
    if let Some(description) = &tool.description {
        payload.insert(
            "description".to_string(),
            Value::String(description.clone())
        );
    }
    payload.insert("parameters".to_string(), tool.parameters_schema.clone());
    payload.insert("strict".to_string(), Value::Bool(strict));
    
    Ok(Value::Object(payload))
}

fn is_strict_compatible_schema(schema: &Value) -> bool {
    let Some(obj) = schema.as_object() else {
        return false;
    };

    if obj.contains_key("anyOf") || obj.contains_key("oneOf") || obj.contains_key("allOf") {
        return false;
    }

    let is_object_schema = is_object_type(obj.get("type"));
    if !is_object_schema {
        if let Some(items) = obj.get("items") {
            return is_strict_compatible_schema(items);
        }
        return true;
    }

    match obj.get("additionalProperties") {
        Some(Value::Bool(false)) => {}
        _ => return false,
    }

    let properties = obj
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let required = obj
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if properties.len() != required.len() {
        return false;
    }

    for key in properties.keys() {
        let present = required
            .iter()
            .filter_map(Value::as_str)
            .any(|required_key| required_key == key);
        if !present {
            return false;
        }
    }

    properties.values().all(is_strict_compatible_schema)
}

fn is_object_type(type_value: Option<&Value>) -> bool {
    match type_value {
        Some(Value::String(value)) => value == "object",
        Some(Value::Array(values)) => values.iter().any(|entry| entry == "object"),
        _ => false,
    }
}

fn map_messages(
    req: &Request,
) -> Result<Vec<Value>, OpenAITranslateError> {
    let mut input_items = Vec::new();
    let mut seen_tool_call_ids: Vec<String> = Vec::new();

    for message in &req.messages {
        let mut message_parts = Vec::new();

        for part in &message.content {
            match part {
                ContentPart::Text { text } => {
                    if message.role == MessageRole::Tool {
                        return Err(OpenAITranslateError{
                            message: "tool role messages cannot contain plain text content".to_string(),
                        });
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
                        return Err(OpenAITranslateError{
                            message: "tool_call content is only valid for assistant role messages".to_string(),
                        });
                    }

                    flush_message_item(&mut input_items, &message.role, &mut message_parts);

                    let arguments =
                        serde_json::to_string(&tool_call.arguments_json).map_err(|e| {
                            OpenAITranslateError {
                                message: format!(
                                    "failed to serialize tool_call arguments for '{}': {e}",
                                    tool_call.name
                                ),
                            }
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
                        return Err(OpenAITranslateError{
                            message: "tool_result content is only valid for tool role messages".to_string(),
                        });
                    }

                    flush_message_item(&mut input_items, &message.role, &mut message_parts);

                    if !seen_tool_call_ids.contains(&tool_result.tool_call_id) {
                        return Err(OpenAITranslateError{
                            message: format!(
                                "tool_result_without_matching_tool_call: {}",
                                tool_result.tool_call_id
                            ),
    });
                    }

                    let output = serialize_tool_result_output(tool_result, req)?;
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

fn serialize_tool_result_output(
    tool_result: &ToolResult,
    req: &Request,
) -> Result<String, OpenAITranslateError> {
    if let Some(raw_provider_content) = &tool_result.raw_provider_content {
        if let Some(raw_text) = raw_provider_content.as_str() {
            return Ok(raw_text.to_string());
        }

        // Maybe add warning that it is ignored as OpenAI expects a string?
    }

    match &tool_result.content {
        ToolResultContent::Text { text } => Ok(text.clone()),
        ToolResultContent::Json { value } => {
            // Warn that tool_result JSON content coerced to string for OpenAI function_call_output
            Ok(stable_json_string(&canonicalize_json(value)))
        }
        ToolResultContent::Parts { parts } => {
            // Warn that tool_result parts content coerced to newline-delimited string for openAI
            // function_call_output

            let mut lines = Vec::new();
            for part in parts {
                match part {
                    ContentPart::Text { text } => lines.push(text.clone()),
                    _ => {
                        return Err(OpenAITranslateError{
                            message: "tool_result parts content for OpenAI must contain only text parts".to_string(),
                        });
                    }
                }
            }
            Ok(lines.join("\n"))
        }
    }
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();

            let mut out = Map::new();
            for key in keys {
                let next = map.get(&key).expect("key collected from object must exist");
                out.insert(key, canonicalize_json(next));
            }

            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json).collect()),
        _ => value.clone(),
    }
}

fn stable_json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

