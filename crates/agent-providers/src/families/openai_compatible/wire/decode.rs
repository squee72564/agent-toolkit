use serde::de::DeserializeOwned;
use serde_json::Value;

use super::types::{
    OpenAiErrorEnvelope, OpenAiFunctionCallOutputItem, OpenAiMessageContentPart,
    OpenAiMessageOutputItem, OpenAiReasoningOutputItem, OpenAiRefusalOutputItem,
    OpenAiResponsesBody,
};
use super::{OpenAiDecodeEnvelope, OpenAiFamilyError};
use crate::error::{AdapterErrorKind, ProviderErrorInfo};
use agent_core::types::{
    AssistantOutput, ContentPart, FinishReason, Response, ResponseFormat, RuntimeWarning, Usage,
};

const WARN_EMPTY_CONTENT: &str = "openai.decode.empty_content";
const WARN_UNKNOWN_OUTPUT_ITEM: &str = "openai.decode.unknown_output_item";
const WARN_UNKNOWN_MESSAGE_PART: &str = "openai.decode.unknown_message_part";
const WARN_INVALID_TOOL_CALL_ARGUMENTS: &str = "openai.decode.invalid_tool_call_arguments";
const WARN_STRUCTURED_OUTPUT_NOT_OBJECT: &str = "openai.decode.structured_output_not_object";
const WARN_STRUCTURED_OUTPUT_PARSE_FAILED: &str = "openai.decode.structured_output_parse_failed";

pub(crate) struct NormalizedOpenAiErrorEnvelope {
    pub message: String,
    pub code: Option<String>,
    pub error_type: Option<String>,
    pub param: Option<String>,
}

pub(crate) fn decode_openai_response(
    payload: &OpenAiDecodeEnvelope,
) -> Result<Response, OpenAiFamilyError> {
    if !payload.body.is_object() {
        return Err(OpenAiFamilyError::decode(
            "response payload must be a JSON object",
        ));
    }

    if let Some(error) = decode_openai_error(&payload.body) {
        return Err(OpenAiFamilyError::upstream(error.message.unwrap_or_else(
            || "openai response reported an error".to_string(),
        )));
    }

    let parsed: OpenAiResponsesBody =
        serde_json::from_value(payload.body.clone()).map_err(|error| {
            OpenAiFamilyError::decode(format!(
                "failed to deserialize OpenAI-family response: {error}"
            ))
        })?;

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

pub(crate) fn decode_openai_error(root: &Value) -> Option<ProviderErrorInfo> {
    let error = parse_openai_error_value(root)?;
    Some(ProviderErrorInfo {
        provider_code: None,
        message: Some(format_openai_error_message(&error)),
        kind: Some(AdapterErrorKind::Upstream),
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

pub(crate) fn parse_openai_error_value(root: &Value) -> Option<NormalizedOpenAiErrorEnvelope> {
    let error_value = root.as_object()?.get("error")?.clone();
    let error: OpenAiErrorEnvelope = serde_json::from_value(error_value).ok()?;
    normalize_error_envelope(&error)
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
        "message" => decode_output_message(
            &deserialize_wire::<OpenAiMessageOutputItem>(item, "output message item")?,
            content,
            warnings,
        ),
        "function_call" => decode_output_tool_call(
            &deserialize_wire::<OpenAiFunctionCallOutputItem>(item, "function_call output item")?,
            content,
            warnings,
        ),
        "reasoning" => {
            let _ = deserialize_wire::<OpenAiReasoningOutputItem>(item, "reasoning output item")?;
            Ok(())
        }
        "refusal" => {
            let item = deserialize_wire::<OpenAiRefusalOutputItem>(item, "refusal output item")?;
            if let Some(text) = extract_refusal_text(item.text.as_deref(), item.refusal.as_deref())
            {
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
    item: &OpenAiMessageOutputItem,
    content: &mut Vec<ContentPart>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiFamilyError> {
    for part in &item.content {
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
            "output_text" | "refusal" | "reasoning" => {
                let part = deserialize_wire::<OpenAiMessageContentPart>(
                    part,
                    "output message content part",
                )?;

                match part {
                    OpenAiMessageContentPart::OutputText { .. } => {
                        let text = part.output_text().unwrap_or_default();
                        if !text.is_empty() {
                            content.push(ContentPart::Text {
                                text: text.to_string(),
                            });
                        }
                    }
                    OpenAiMessageContentPart::Reasoning => {}
                    OpenAiMessageContentPart::Refusal { .. } => {
                        if let Some(text) = part.refusal_text() {
                            content.push(ContentPart::Text {
                                text: text.to_string(),
                            });
                        }
                    }
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

    Ok(())
}

fn decode_required_non_empty_str<'a>(
    value: Option<&'a str>,
    missing_message: &str,
    blank_message: &str,
) -> Result<&'a str, OpenAiFamilyError> {
    let value = value.ok_or_else(|| OpenAiFamilyError::decode(missing_message))?;

    if value.trim().is_empty() {
        return Err(OpenAiFamilyError::decode(blank_message));
    }

    Ok(value)
}

fn decode_output_tool_call(
    item: &OpenAiFunctionCallOutputItem,
    content: &mut Vec<ContentPart>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiFamilyError> {
    let call_id = decode_required_non_empty_str(
        item.call_id.as_deref(),
        "function_call output item missing call_id",
        "function_call output item call_id must not be empty",
    )?;
    let name = decode_required_non_empty_str(
        item.name.as_deref(),
        "function_call output item missing name",
        "function_call output item name must not be empty",
    )?;
    let arguments = decode_required_non_empty_str(
        item.arguments.as_deref(),
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

fn extract_refusal_text(text: Option<&str>, refusal: Option<&str>) -> Option<String> {
    if let Some(text) = text {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(text) = refusal {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn deserialize_wire<T>(value: &Value, label: &str) -> Result<T, OpenAiFamilyError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.clone())
        .map_err(|error| OpenAiFamilyError::decode(format!("invalid {label}: {error}")))
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
