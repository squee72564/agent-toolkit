use serde_json::{Map, Value};

use super::types::{OpenAiErrorEnvelope, OpenAiResponsesBody};
use super::{OpenAiDecodeEnvelope, OpenAiFamilyError};
use agent_core::types::{
    AssistantOutput, ContentPart, FinishReason, Response, ResponseFormat, RuntimeWarning, Usage,
};

const WARN_EMPTY_CONTENT: &str = "openai.decode.empty_content";
const WARN_UNKNOWN_OUTPUT_ITEM: &str = "openai.decode.unknown_output_item";
const WARN_UNKNOWN_MESSAGE_PART: &str = "openai.decode.unknown_message_part";
const WARN_INVALID_TOOL_CALL_ARGUMENTS: &str = "openai.decode.invalid_tool_call_arguments";
const WARN_STRUCTURED_OUTPUT_NOT_OBJECT: &str = "openai.decode.structured_output_not_object";
const WARN_STRUCTURED_OUTPUT_PARSE_FAILED: &str = "openai.decode.structured_output_parse_failed";

struct NormalizedOpenAiErrorEnvelope {
    message: String,
    code: Option<String>,
    error_type: Option<String>,
    param: Option<String>,
}

pub(crate) fn decode_openai_response(
    payload: &OpenAiDecodeEnvelope,
) -> Result<Response, OpenAiFamilyError> {
    let parsed: OpenAiResponsesBody =
        serde_json::from_value(payload.body.clone()).map_err(|error| {
            OpenAiFamilyError::decode(format!(
                "failed to deserialize OpenAI-family response: {error}"
            ))
        })?;
    if !payload.body.is_object() {
        return Err(OpenAiFamilyError::decode(
            "response payload must be a JSON object",
        ));
    }

    if let Some(error) = parsed.error.as_ref().and_then(normalize_error_envelope) {
        return Err(OpenAiFamilyError::upstream(format_openai_error_message(
            &error,
        )));
    }

    let status = parsed
        .status
        .as_deref()
        .ok_or_else(|| OpenAiFamilyError::decode("openai response missing status"))?;

    let model = parsed
        .model
        .unwrap_or_else(|| "<unknown-model>".to_string());

    let mut content = Vec::new();
    let mut warnings = Vec::new();

    if let Some(output_items) = parsed.output.as_ref().and_then(Value::as_array) {
        for item in output_items {
            decode_output_item(item, &mut content, &mut warnings)?;
        }
    }

    if content.is_empty() {
        push_warning(
            &mut warnings,
            WARN_EMPTY_CONTENT,
            "openai response produced no decodable content parts",
        );
    }

    let structured_output =
        decode_structured_output(&payload.requested_response_format, &content, &mut warnings);

    let usage = parsed.usage.map(Usage::from).unwrap_or_default();

    let incomplete_reason = parsed
        .incomplete_details
        .as_ref()
        .and_then(|details| details.reason.as_deref());

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

fn format_openai_error_message(envelope: &NormalizedOpenAiErrorEnvelope) -> String {
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

fn normalize_error_envelope(error: &OpenAiErrorEnvelope) -> Option<NormalizedOpenAiErrorEnvelope> {
    let message = value_to_string(error.message.as_ref())
        .unwrap_or_else(|| "openai response reported an error".to_string());
    let code = value_to_string(error.code.as_ref());
    let error_type = value_to_string(error.error_type.as_ref());
    let param = value_to_string(error.param.as_ref());

    if !message.trim().is_empty() || code.is_some() || error_type.is_some() || param.is_some() {
        Some(NormalizedOpenAiErrorEnvelope {
            message,
            code,
            error_type,
            param,
        })
    } else {
        None
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
) -> Result<(), OpenAiFamilyError> {
    let item_obj = item
        .as_object()
        .ok_or_else(|| OpenAiFamilyError::decode("output item must be an object"))?;
    let item_type = item_obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenAiFamilyError::decode("output item missing type"))?;

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
                WARN_UNKNOWN_OUTPUT_ITEM,
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
) -> Result<(), OpenAiFamilyError> {
    if let Some(parts) = item_obj.get("content").and_then(Value::as_array) {
        for part in parts {
            let Some(part_obj) = part.as_object() else {
                return Err(OpenAiFamilyError::decode(
                    "output message content part must be an object",
                ));
            };

            let Some(part_type) = part_obj.get("type").and_then(Value::as_str) else {
                return Err(OpenAiFamilyError::decode(
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
                        WARN_UNKNOWN_MESSAGE_PART,
                        format!("ignored unknown output message content part type: {other}"),
                    );
                }
            }
        }
    }

    Ok(())
}

fn decode_required_non_empty_str<'a>(
    item_obj: &'a Map<String, Value>,
    key: &str,
    missing_message: &str,
    blank_message: &str,
) -> Result<&'a str, OpenAiFamilyError> {
    let value = item_obj
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| OpenAiFamilyError::decode(missing_message))?;

    if value.trim().is_empty() {
        return Err(OpenAiFamilyError::decode(blank_message));
    }

    Ok(value)
}

fn decode_output_tool_call(
    item_obj: &Map<String, Value>,
    content: &mut Vec<ContentPart>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiFamilyError> {
    let call_id = decode_required_non_empty_str(
        item_obj,
        "call_id",
        "function_call output item missing call_id",
        "function_call output item call_id must not be empty",
    )?;
    let name = decode_required_non_empty_str(
        item_obj,
        "name",
        "function_call output item missing name",
        "function_call output item name must not be empty",
    )?;
    let arguments = decode_required_non_empty_str(
        item_obj,
        "arguments",
        "function_call output item missing arguments",
        "function_call output item arguments must not be empty",
    )?;

    let arguments_json = match serde_json::from_str::<Value>(arguments) {
        Ok(value) => value,
        Err(_) => {
            push_warning(
                warnings,
                WARN_INVALID_TOOL_CALL_ARGUMENTS,
                format!(
                    "tool call arguments for '{name}' are not valid JSON; preserving raw string"
                ),
            );
            Value::String(arguments.to_string())
        }
    };

    content.push(ContentPart::tool_call(
        call_id.to_string(),
        name.to_string(),
        arguments_json,
    ));

    Ok(())
}

fn extract_refusal_text(obj: &Map<String, Value>) -> Option<String> {
    if let Some(text) = obj.get("text").and_then(Value::as_str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(text) = obj.get("refusal").and_then(Value::as_str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
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
                        WARN_STRUCTURED_OUTPUT_NOT_OBJECT,
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
                WARN_STRUCTURED_OUTPUT_PARSE_FAILED,
                "unable to parse structured output JSON from decoded text",
            );
            None
        }
    }
}

fn map_finish_reason(
    status: &str,
    incomplete_reason: Option<&str>,
    content: &[ContentPart],
) -> Result<FinishReason, OpenAiFamilyError> {
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
        "cancelled" => Err(OpenAiFamilyError::decode(
            "openai response status is cancelled",
        )),
        "failed" => Err(OpenAiFamilyError::decode(
            "openai response status is failed",
        )),
        "in_progress" | "queued" => Err(OpenAiFamilyError::decode(format!(
            "openai response status is non-terminal: {status}"
        ))),
        other => Err(OpenAiFamilyError::decode(format!(
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
