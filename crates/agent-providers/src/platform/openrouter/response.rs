use agent_core::{
    AssistantOutput, ContentPart, FinishReason, Response, ResponseFormat, RuntimeWarning, Usage,
};
use serde_json::{Map, Value};

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_spec::decode::decode_openai_response;
use crate::openai_spec::{OpenAiDecodeEnvelope, OpenAiSpecError, OpenAiSpecErrorKind};

const WARN_FALLBACK_CHAT_COMPLETIONS: &str = "openrouter.decode.fallback_chat_completions";
const WARN_UNKNOWN_CONTENT_PART: &str = "openrouter.decode.unknown_message_part";
const WARN_INVALID_TOOL_CALL_ARGUMENTS: &str = "openrouter.decode.invalid_tool_call_arguments";
const WARN_MISSING_TOOL_CALL_NAME: &str = "openrouter.decode.missing_tool_call_name";
const WARN_MISSING_TOOL_CALL_ID: &str = "openrouter.decode.missing_tool_call_id";
const WARN_EMPTY_CONTENT: &str = "openrouter.decode.empty_content";
const WARN_UNKNOWN_FINISH_REASON: &str = "openrouter.decode.unknown_finish_reason";
const WARN_STRUCTURED_OUTPUT_PARSE_FAILED: &str =
    "openrouter.decode.structured_output_parse_failed";
const WARN_STRUCTURED_OUTPUT_NOT_OBJECT: &str = "openrouter.decode.structured_output_not_object";

pub(crate) fn decode_response_json(
    body: Value,
    requested_format: &ResponseFormat,
) -> Result<Response, AdapterError> {
    let payload = OpenAiDecodeEnvelope {
        body,
        requested_response_format: requested_format.clone(),
    };

    match decode_openai_response(&payload) {
        Ok(response) => Ok(response),
        Err(openai_decode_error) => {
            if !should_attempt_openrouter_fallback(&openai_decode_error) {
                return Err(map_openrouter_decode_error(openai_decode_error));
            }

            match decode_openrouter_chat_completions_response(&payload) {
                Ok(response) => Ok(response),
                Err(fallback_error) => Err(map_openrouter_decode_error(OpenAiSpecError::decode(
                    format!(
                        "openai-compatible decode failed: {}; openrouter fallback decode failed: {}",
                        openai_decode_error.message(),
                        fallback_error.message()
                    ),
                ))),
            }
        }
    }
}

fn map_openrouter_decode_error(error: OpenAiSpecError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::OpenRouter,
        AdapterOperation::DecodeResponse,
        message,
        error,
    )
}

fn map_spec_error_kind(kind: OpenAiSpecErrorKind) -> AdapterErrorKind {
    match kind {
        OpenAiSpecErrorKind::Validation => AdapterErrorKind::Validation,
        OpenAiSpecErrorKind::Encode => AdapterErrorKind::Encode,
        OpenAiSpecErrorKind::Decode => AdapterErrorKind::Decode,
        OpenAiSpecErrorKind::Upstream => AdapterErrorKind::Upstream,
        OpenAiSpecErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        OpenAiSpecErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}

fn should_attempt_openrouter_fallback(error: &OpenAiSpecError) -> bool {
    matches!(
        error.kind(),
        OpenAiSpecErrorKind::Decode | OpenAiSpecErrorKind::ProtocolViolation
    )
}

fn decode_openrouter_chat_completions_response(
    payload: &OpenAiDecodeEnvelope,
) -> Result<Response, OpenAiSpecError> {
    let root = payload.body.as_object().ok_or_else(|| {
        OpenAiSpecError::decode("openrouter response payload must be a JSON object")
    })?;

    if let Some(error_obj) = root.get("error").and_then(Value::as_object) {
        return Err(OpenAiSpecError::upstream(format_openrouter_error_message(
            error_obj,
        )));
    }

    let choices = root
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| OpenAiSpecError::decode("openrouter response missing choices array"))?;
    if choices.is_empty() {
        return Err(OpenAiSpecError::decode(
            "openrouter response choices array must not be empty",
        ));
    }

    let choice = choices[0].as_object().ok_or_else(|| {
        OpenAiSpecError::decode("openrouter response choices[0] must be an object")
    })?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| OpenAiSpecError::decode("openrouter response choice missing message"))?;

    let model = root
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("<unknown-model>")
        .to_string();

    let mut warnings = vec![RuntimeWarning {
        code: WARN_FALLBACK_CHAT_COMPLETIONS.to_string(),
        message: "decoded OpenRouter response using chat-completions fallback".to_string(),
    }];
    let mut content = Vec::new();

    decode_openrouter_message_content(message.get("content"), &mut content, &mut warnings)?;
    decode_openrouter_refusal_content(message.get("refusal"), &mut content);
    decode_openrouter_tool_calls(message.get("tool_calls"), &mut content, &mut warnings)?;

    if content.is_empty() {
        warnings.push(RuntimeWarning {
            code: WARN_EMPTY_CONTENT.to_string(),
            message: "openrouter response produced no decodable content parts".to_string(),
        });
    }

    let finish_reason = map_openrouter_finish_reason(
        choice.get("finish_reason").and_then(Value::as_str),
        &content,
        &mut warnings,
    );
    let usage = decode_openrouter_usage(root.get("usage"));
    let structured_output = decode_openrouter_structured_output(
        &payload.requested_response_format,
        &content,
        &mut warnings,
    );

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

fn format_openrouter_error_message(error_obj: &Map<String, Value>) -> String {
    let message = value_to_string(error_obj.get("message"))
        .unwrap_or_else(|| "openrouter response reported an error".to_string());
    let code = value_to_string(error_obj.get("code"));

    match code {
        Some(code) => format!("openrouter error: {message} [code={code}]"),
        None => format!("openrouter error: {message}"),
    }
}

fn decode_openrouter_message_content(
    value: Option<&Value>,
    content: &mut Vec<ContentPart>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiSpecError> {
    let Some(value) = value else {
        return Ok(());
    };

    match value {
        Value::Null => Ok(()),
        Value::String(text) => {
            if !text.is_empty() {
                content.push(ContentPart::Text { text: text.clone() });
            }
            Ok(())
        }
        Value::Array(parts) => {
            for part in parts {
                match part {
                    Value::String(text) => {
                        if !text.is_empty() {
                            content.push(ContentPart::Text { text: text.clone() });
                        }
                    }
                    Value::Object(part_obj) => {
                        let part_type = part_obj.get("type").and_then(Value::as_str).unwrap_or("");
                        match part_type {
                            "text" | "output_text" => {
                                if let Some(text) = part_obj.get("text").and_then(Value::as_str)
                                    && !text.is_empty()
                                {
                                    content.push(ContentPart::Text {
                                        text: text.to_string(),
                                    });
                                }
                            }
                            "refusal" => {
                                if let Some(text) = part_obj
                                    .get("refusal")
                                    .and_then(Value::as_str)
                                    .or_else(|| part_obj.get("text").and_then(Value::as_str))
                                    && !text.is_empty()
                                {
                                    content.push(ContentPart::Text {
                                        text: text.to_string(),
                                    });
                                }
                            }
                            other => {
                                warnings.push(RuntimeWarning {
                                    code: WARN_UNKNOWN_CONTENT_PART.to_string(),
                                    message: format!(
                                        "ignored unsupported openrouter message content part type: {other}"
                                    ),
                                });
                            }
                        }
                    }
                    _ => {
                        warnings.push(RuntimeWarning {
                            code: WARN_UNKNOWN_CONTENT_PART.to_string(),
                            message: "ignored unsupported openrouter message content part"
                                .to_string(),
                        });
                    }
                }
            }
            Ok(())
        }
        _ => Err(OpenAiSpecError::decode(
            "openrouter response message content must be string, array, or null",
        )),
    }
}

fn decode_openrouter_refusal_content(value: Option<&Value>, content: &mut Vec<ContentPart>) {
    if let Some(text) = value.and_then(Value::as_str)
        && !text.is_empty()
    {
        content.push(ContentPart::Text {
            text: text.to_string(),
        });
    }
}

fn decode_openrouter_tool_calls(
    value: Option<&Value>,
    content: &mut Vec<ContentPart>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiSpecError> {
    let Some(value) = value else {
        return Ok(());
    };

    let tool_calls = value.as_array().ok_or_else(|| {
        OpenAiSpecError::decode("openrouter response tool_calls must be an array")
    })?;

    for (index, tool_call) in tool_calls.iter().enumerate() {
        let tool_call = tool_call.as_object().ok_or_else(|| {
            OpenAiSpecError::decode("openrouter response tool_calls entries must be objects")
        })?;
        let function = tool_call
            .get("function")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                OpenAiSpecError::decode("openrouter response tool_call missing function object")
            })?;

        let Some(name) = function.get("name").and_then(Value::as_str) else {
            warnings.push(RuntimeWarning {
                code: WARN_MISSING_TOOL_CALL_NAME.to_string(),
                message: format!("ignored openrouter tool call at index {index} without a name"),
            });
            continue;
        };
        if name.trim().is_empty() {
            warnings.push(RuntimeWarning {
                code: WARN_MISSING_TOOL_CALL_NAME.to_string(),
                message: format!("ignored openrouter tool call at index {index} with blank name"),
            });
            continue;
        }

        let id = match tool_call.get("id").and_then(Value::as_str) {
            Some(id) if !id.trim().is_empty() => id.to_string(),
            _ => {
                warnings.push(RuntimeWarning {
                    code: WARN_MISSING_TOOL_CALL_ID.to_string(),
                    message: format!(
                        "openrouter tool call at index {index} missing id; generated synthetic id"
                    ),
                });
                format!("openrouter_tool_call_{index}")
            }
        };

        let arguments_json = match function.get("arguments").and_then(Value::as_str) {
            Some(arguments) => match serde_json::from_str(arguments) {
                Ok(value) => value,
                Err(_) => {
                    warnings.push(RuntimeWarning {
                        code: WARN_INVALID_TOOL_CALL_ARGUMENTS.to_string(),
                        message: format!(
                            "openrouter tool call '{name}' arguments were invalid JSON; preserved raw string"
                        ),
                    });
                    Value::String(arguments.to_string())
                }
            },
            None => Value::Object(Map::new()),
        };

        content.push(ContentPart::tool_call(id, name.to_string(), arguments_json));
    }

    Ok(())
}

fn map_openrouter_finish_reason(
    reason: Option<&str>,
    content: &[ContentPart],
    warnings: &mut Vec<RuntimeWarning>,
) -> FinishReason {
    match reason {
        Some("stop") | Some("end_turn") => FinishReason::Stop,
        Some("length") | Some("max_tokens") => FinishReason::Length,
        Some("tool_calls") | Some("tool_use") => FinishReason::ToolCalls,
        Some("content_filter") => FinishReason::ContentFilter,
        Some("error") => FinishReason::Error,
        Some(other) => {
            warnings.push(RuntimeWarning {
                code: WARN_UNKNOWN_FINISH_REASON.to_string(),
                message: format!("unknown openrouter finish_reason '{other}' mapped to Other"),
            });
            FinishReason::Other
        }
        None => {
            if content
                .iter()
                .any(|part| matches!(part, ContentPart::ToolCall { .. }))
            {
                FinishReason::ToolCalls
            } else {
                FinishReason::Stop
            }
        }
    }
}

fn decode_openrouter_usage(value: Option<&Value>) -> Usage {
    let usage = value.and_then(Value::as_object);
    Usage {
        input_tokens: usage
            .and_then(|usage| usage.get("prompt_tokens"))
            .and_then(Value::as_u64),
        output_tokens: usage
            .and_then(|usage| usage.get("completion_tokens"))
            .and_then(Value::as_u64),
        total_tokens: usage
            .and_then(|usage| usage.get("total_tokens"))
            .and_then(Value::as_u64),
        cached_input_tokens: usage
            .and_then(|usage| usage.get("prompt_tokens_details"))
            .and_then(Value::as_object)
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64),
    }
}

fn decode_openrouter_structured_output(
    requested_format: &ResponseFormat,
    content: &[ContentPart],
    warnings: &mut Vec<RuntimeWarning>,
) -> Option<Value> {
    match requested_format {
        ResponseFormat::Text => None,
        ResponseFormat::JsonObject | ResponseFormat::JsonSchema { .. } => {
            let text = content.iter().find_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })?;

            match serde_json::from_str::<Value>(text) {
                Ok(Value::Object(object)) => Some(Value::Object(object)),
                Ok(_) => {
                    warnings.push(RuntimeWarning {
                        code: WARN_STRUCTURED_OUTPUT_NOT_OBJECT.to_string(),
                        message: "openrouter structured output was valid JSON but not an object"
                            .to_string(),
                    });
                    None
                }
                Err(error) => {
                    warnings.push(RuntimeWarning {
                        code: WARN_STRUCTURED_OUTPUT_PARSE_FAILED.to_string(),
                        message: format!(
                            "failed to parse openrouter structured output as JSON object: {error}"
                        ),
                    });
                    None
                }
            }
        }
    }
}

fn value_to_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(boolean)) => Some(boolean.to_string()),
        _ => None,
    }
}
