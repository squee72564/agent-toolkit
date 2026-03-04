use serde_json::{Map, Value};

use agent_core::types::{
    AssistantOutput, ContentPart, FinishReason, Response, ResponseFormat, RuntimeWarning, ToolCall,
    Usage,
};

use super::{OpenAiDecodeEnvelope, OpenAiErrorEnvelope, OpenAiSpecError};

pub(crate) fn decode_openai_response(
    payload: &OpenAiDecodeEnvelope,
) -> Result<Response, OpenAiSpecError> {
    let root = payload
        .body
        .as_object()
        .ok_or_else(|| OpenAiSpecError::decode("response payload must be a JSON object"))?;

    if let Some(error) = parse_openai_error_value(root) {
        return Err(OpenAiSpecError::upstream(format_openai_error_message(
            &error,
        )));
    }

    let status = root
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenAiSpecError::decode("openai response missing status"))?;

    let model = root
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("<unknown-model>")
        .to_string();

    let mut content = Vec::new();
    let mut warnings = Vec::new();

    let output_items = root
        .get("output")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for item in output_items {
        decode_output_item(&item, &mut content, &mut warnings)?;
    }

    if content.is_empty() {
        push_warning(
            &mut warnings,
            "openai.decode.empty_content",
            "openai response produced no decodable content parts",
        );
    }

    let structured_output =
        decode_structured_output(&payload.requested_response_format, &content, &mut warnings);

    let usage = decode_usage(root.get("usage"));

    let incomplete_reason = root
        .get("incomplete_details")
        .and_then(Value::as_object)
        .and_then(|details| details.get("reason"))
        .and_then(Value::as_str);

    let finish_reason = map_finish_reason(status, incomplete_reason, &content)?;

    Ok(Response {
        output: AssistantOutput {
            content,
            structured_output,
        },
        usage,
        model,
        raw_provider_response: None,
        finish_reason,
        warnings,
    })
}

fn parse_openai_error_value(root: &Map<String, Value>) -> Option<OpenAiErrorEnvelope> {
    let error = root.get("error")?.as_object()?;
    let message = value_to_string(error.get("message"))
        .unwrap_or_else(|| "openai response reported an error".to_string());

    Some(OpenAiErrorEnvelope {
        message,
        code: value_to_string(error.get("code")),
        error_type: value_to_string(error.get("type")),
        param: value_to_string(error.get("param")),
    })
}

pub(crate) fn format_openai_error_message(envelope: &OpenAiErrorEnvelope) -> String {
    let mut context = Vec::new();

    if let Some(code) = &envelope.code {
        context.push(format!("code={code}"));
    }
    if let Some(error_type) = &envelope.error_type {
        context.push(format!("type={error_type}"));
    }
    if let Some(param) = &envelope.param {
        context.push(format!("param={param}"));
    }

    if context.is_empty() {
        format!("openai error: {}", envelope.message)
    } else {
        format!(
            "openai error: {} [{}]",
            envelope.message,
            context.join(", ")
        )
    }
}

fn value_to_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(flag)) => Some(flag.to_string()),
        _ => None,
    }
}

fn decode_output_item(
    item: &Value,
    content: &mut Vec<ContentPart>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiSpecError> {
    let item_obj = item
        .as_object()
        .ok_or_else(|| OpenAiSpecError::decode("output item must be an object"))?;
    let item_type = item_obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenAiSpecError::decode("output item missing type"))?;

    match item_type {
        "message" => decode_output_message(item_obj, content, warnings),
        "function_call" => decode_output_tool_call(item_obj, content, warnings),
        "reasoning" => Ok(()),
        "refusal" => {
            if let Some(text) = extract_refusal_text(item_obj) {
                content.push(ContentPart::Text { text });
            }
            Ok(())
        }
        other => {
            push_warning(
                warnings,
                "openai.decode.unknown_output_item",
                format!("ignored unknown output item type: {other}"),
            );
            Ok(())
        }
    }
}

fn decode_output_message(
    item_obj: &Map<String, Value>,
    content: &mut Vec<ContentPart>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiSpecError> {
    let parts = item_obj
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for part in parts {
        let Some(part_obj) = part.as_object() else {
            return Err(OpenAiSpecError::decode(
                "output message content part must be an object",
            ));
        };

        let Some(part_type) = part_obj.get("type").and_then(Value::as_str) else {
            return Err(OpenAiSpecError::decode(
                "output message content part missing type",
            ));
        };

        match part_type {
            "output_text" => {
                let text = part_obj
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !text.is_empty() {
                    content.push(ContentPart::Text {
                        text: text.to_string(),
                    });
                }
            }
            "reasoning" => {}
            "refusal" => {
                if let Some(text) = extract_refusal_text(part_obj) {
                    content.push(ContentPart::Text { text });
                }
            }
            other => {
                push_warning(
                    warnings,
                    "openai.decode.unknown_message_part",
                    format!("ignored unknown output message content part type: {other}"),
                );
            }
        }
    }

    Ok(())
}

fn decode_output_tool_call(
    item_obj: &Map<String, Value>,
    content: &mut Vec<ContentPart>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiSpecError> {
    let call_id = item_obj
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenAiSpecError::decode("function_call output item missing call_id"))?;
    let name = item_obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenAiSpecError::decode("function_call output item missing name"))?;
    let arguments = item_obj
        .get("arguments")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenAiSpecError::decode("function_call output item missing arguments"))?;

    let arguments_json = match serde_json::from_str::<Value>(arguments) {
        Ok(value) => value,
        Err(_) => {
            push_warning(
                warnings,
                "openai.decode.invalid_tool_call_arguments",
                format!(
                    "tool call arguments for '{name}' are not valid JSON; preserving raw string"
                ),
            );
            Value::String(arguments.to_string())
        }
    };

    content.push(ContentPart::ToolCall {
        tool_call: ToolCall {
            id: call_id.to_string(),
            name: name.to_string(),
            arguments_json,
        },
    });

    Ok(())
}

fn extract_refusal_text(obj: &Map<String, Value>) -> Option<String> {
    if let Some(text) = obj.get("text").and_then(Value::as_str) {
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }

    if let Some(text) = obj.get("refusal").and_then(Value::as_str) {
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }

    None
}

fn decode_structured_output(
    requested_response_format: &ResponseFormat,
    content: &[ContentPart],
    warnings: &mut Vec<RuntimeWarning>,
) -> Option<Value> {
    if matches!(requested_response_format, ResponseFormat::Text) {
        return None;
    }

    let joined_text = content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if joined_text.trim().is_empty() {
        return None;
    }

    match serde_json::from_str::<Value>(&joined_text) {
        Ok(parsed) => match requested_response_format {
            ResponseFormat::JsonObject => {
                if parsed.is_object() {
                    Some(parsed)
                } else {
                    push_warning(
                        warnings,
                        "openai.decode.structured_output_not_object",
                        "structured output is valid JSON but not a JSON object",
                    );
                    None
                }
            }
            ResponseFormat::JsonSchema { .. } => Some(parsed),
            ResponseFormat::Text => None,
        },
        Err(_) => {
            push_warning(
                warnings,
                "openai.decode.structured_output_parse_failed",
                "unable to parse structured output JSON from decoded text",
            );
            None
        }
    }
}

fn decode_usage(usage: Option<&Value>) -> Usage {
    let Some(usage_obj) = usage.and_then(Value::as_object) else {
        return Usage::default();
    };

    let input_tokens = usage_obj.get("input_tokens").and_then(Value::as_u64);
    let output_tokens = usage_obj.get("output_tokens").and_then(Value::as_u64);
    let total_tokens = usage_obj.get("total_tokens").and_then(Value::as_u64);
    let cached_input_tokens = usage_obj
        .get("input_tokens_details")
        .and_then(Value::as_object)
        .and_then(|details| details.get("cached_tokens"))
        .and_then(Value::as_u64);

    Usage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
    }
}

fn map_finish_reason(
    status: &str,
    incomplete_reason: Option<&str>,
    content: &[ContentPart],
) -> Result<FinishReason, OpenAiSpecError> {
    match status {
        "completed" => {
            if should_finish_with_tool_calls(content) {
                Ok(FinishReason::ToolCalls)
            } else {
                Ok(FinishReason::Stop)
            }
        }
        "incomplete" => match incomplete_reason {
            Some("max_output_tokens") | Some("max_tokens") => Ok(FinishReason::Length),
            Some("content_filter") => Ok(FinishReason::ContentFilter),
            Some(_) => Ok(FinishReason::Other),
            None => Ok(FinishReason::Other),
        },
        "cancelled" => Err(OpenAiSpecError::decode(
            "openai response status is cancelled",
        )),
        "failed" => Err(OpenAiSpecError::decode("openai response status is failed")),
        "in_progress" | "queued" => Err(OpenAiSpecError::decode(format!(
            "openai response status is non-terminal: {status}"
        ))),
        other => Err(OpenAiSpecError::decode(format!(
            "unknown openai response status: {other}"
        ))),
    }
}

fn should_finish_with_tool_calls(content: &[ContentPart]) -> bool {
    let mut saw_tool_call = false;
    let mut saw_text_after_tool_call = false;

    for part in content {
        match part {
            ContentPart::ToolCall { .. } => saw_tool_call = true,
            ContentPart::Text { text } if saw_tool_call && !text.trim().is_empty() => {
                saw_text_after_tool_call = true;
            }
            _ => {}
        }
    }

    saw_tool_call && !saw_text_after_tool_call
}

fn push_warning(warnings: &mut Vec<RuntimeWarning>, code: &str, message: impl Into<String>) {
    warnings.push(RuntimeWarning {
        code: code.to_string(),
        message: message.into(),
    });
}
