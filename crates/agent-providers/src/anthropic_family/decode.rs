use serde_json::{Map, Value};

use crate::error::{AdapterErrorKind, ProviderErrorInfo};
use agent_core::types::{
    AssistantOutput, ContentPart, FinishReason, Response, ResponseFormat, RuntimeWarning, Usage,
};

use super::schema_rules::{canonicalize_json, extract_first_json_object, stable_json_string};
use super::{AnthropicDecodeEnvelope, AnthropicErrorEnvelope, AnthropicFamilyError};

const WARN_UNKNOWN_CONTENT_BLOCK_MAPPED: &str =
    "anthropic.decode.unknown_content_block_mapped_to_text";
const WARN_THINKING_SKIPPED: &str = "anthropic.decode.unrepresentable_thinking_skipped";
const WARN_EMPTY_OUTPUT: &str = "anthropic.decode.empty_output";
const WARN_UNKNOWN_STOP_REASON: &str = "anthropic.decode.unknown_stop_reason";
const WARN_USAGE_MISSING: &str = "anthropic.decode.usage_missing";
const WARN_USAGE_PARTIAL: &str = "anthropic.decode.usage_partial";
const WARN_USAGE_OVERFLOW: &str = "anthropic.decode.usage_overflow";
const WARN_STRUCTURED_OUTPUT_PARSE_FAILED: &str = "anthropic.decode.structured_output_parse_failed";

pub(crate) fn decode_anthropic_response(
    payload: &AnthropicDecodeEnvelope,
) -> Result<Response, AnthropicFamilyError> {
    let root = payload
        .body
        .as_object()
        .ok_or_else(|| AnthropicFamilyError::decode("response payload must be a JSON object"))?;

    if let Some(error) = decode_anthropic_error(&payload.body) {
        return Err(AnthropicFamilyError::upstream(
            error
                .message
                .unwrap_or_else(|| "anthropic response reported an error".to_string()),
        ));
    }

    let model = root
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("<unknown-model>")
        .to_string();

    let role = root
        .get("role")
        .and_then(Value::as_str)
        .ok_or_else(|| AnthropicFamilyError::decode("anthropic response missing role"))?;
    if role != "assistant" {
        return Err(AnthropicFamilyError::decode(format!(
            "anthropic response role must be assistant, got {role}",
        )));
    }

    let stop_reason = root
        .get("stop_reason")
        .and_then(Value::as_str)
        .ok_or_else(|| AnthropicFamilyError::decode("anthropic response missing stop_reason"))?;
    if stop_reason.is_empty() {
        return Err(AnthropicFamilyError::decode(
            "anthropic stop_reason must not be empty",
        ));
    }

    let content_blocks = root
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| AnthropicFamilyError::decode("anthropic response missing content array"))?;

    let mut warnings = Vec::new();
    let mut content = Vec::new();
    let mut text_blocks = Vec::new();

    for block in content_blocks {
        decode_content_block(block, &mut content, &mut text_blocks, &mut warnings)?;
    }

    if content.is_empty() {
        push_warning(
            &mut warnings,
            WARN_EMPTY_OUTPUT,
            "anthropic response produced no decodable content parts",
        );
    }

    let structured_output = decode_structured_output(
        &payload.requested_response_format,
        &text_blocks,
        &mut warnings,
    );
    let usage = decode_usage(root.get("usage"), &mut warnings)?;
    let finish_reason = map_finish_reason(stop_reason, &mut warnings);

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

pub(crate) fn decode_anthropic_error(body: &Value) -> Option<ProviderErrorInfo> {
    let root = body.as_object()?;
    let error = parse_anthropic_error_value(root)?;
    Some(ProviderErrorInfo {
        provider_code: None,
        message: Some(format_anthropic_error_message(&error)),
        kind: Some(AdapterErrorKind::Upstream),
    })
}

pub(crate) fn parse_anthropic_error_value(
    root: &Map<String, Value>,
) -> Option<AnthropicErrorEnvelope> {
    if let Some(top_level_type) = root.get("type").and_then(Value::as_str)
        && top_level_type != "error"
    {
        return None;
    }

    let error_obj = root.get("error")?.as_object()?;
    let message = value_to_string(error_obj.get("message"))
        .unwrap_or_else(|| "anthropic response reported an error".to_string());
    let error_type = value_to_string(error_obj.get("type"));
    let request_id = value_to_string(root.get("request_id"));

    Some(AnthropicErrorEnvelope {
        message,
        error_type,
        request_id,
    })
}

pub(crate) fn format_anthropic_error_message(envelope: &AnthropicErrorEnvelope) -> String {
    let mut context = Vec::new();
    if let Some(error_type) = &envelope.error_type {
        context.push(format!("type={error_type}"));
    }
    if let Some(request_id) = &envelope.request_id {
        context.push(format!("request_id={request_id}"));
    }

    if context.is_empty() {
        format!("anthropic error: {}", envelope.message)
    } else {
        format!(
            "anthropic error: {} [{}]",
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

fn decode_content_block(
    block: &Value,
    content: &mut Vec<ContentPart>,
    text_blocks: &mut Vec<String>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), AnthropicFamilyError> {
    let block_obj = block
        .as_object()
        .ok_or_else(|| AnthropicFamilyError::decode("anthropic content block must be object"))?;
    let block_type = block_obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| AnthropicFamilyError::decode("anthropic content block missing type"))?;

    match block_type {
        "text" => {
            let text = block_obj
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| AnthropicFamilyError::decode("text content block missing text"))?;
            text_blocks.push(text.to_string());
            content.push(ContentPart::Text {
                text: text.to_string(),
            });
            Ok(())
        }
        "tool_use" => {
            let id = block_obj
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| AnthropicFamilyError::decode("tool_use block missing id"))?;
            let name = block_obj
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| AnthropicFamilyError::decode("tool_use block missing name"))?;
            let input = block_obj
                .get("input")
                .ok_or_else(|| AnthropicFamilyError::decode("tool_use block missing input"))?
                .clone();
            if !input.is_object() {
                return Err(AnthropicFamilyError::decode(
                    "tool_use input must be a JSON object",
                ));
            }

            content.push(ContentPart::tool_call(
                id.to_string(),
                name.to_string(),
                input,
            ));
            Ok(())
        }
        "thinking" | "redacted_thinking" => {
            push_warning(
                warnings,
                WARN_THINKING_SKIPPED,
                format!(
                    "anthropic content block type '{block_type}' is not representable and was skipped",
                ),
            );
            Ok(())
        }
        _ => {
            push_warning(
                warnings,
                WARN_UNKNOWN_CONTENT_BLOCK_MAPPED,
                format!(
                    "anthropic content block type '{block_type}' mapped to canonical text via JSON",
                ),
            );
            content.push(ContentPart::Text {
                text: stable_json_string(&canonicalize_json(block)),
            });
            Ok(())
        }
    }
}

fn decode_structured_output(
    requested_response_format: &ResponseFormat,
    text_blocks: &[String],
    warnings: &mut Vec<RuntimeWarning>,
) -> Option<Value> {
    match requested_response_format {
        ResponseFormat::Text => None,
        ResponseFormat::JsonSchema { .. } => {
            let first_text = text_blocks.first()?;
            match serde_json::from_str::<Value>(first_text) {
                Ok(parsed) => Some(parsed),
                Err(error) => {
                    push_warning(
                        warnings,
                        WARN_STRUCTURED_OUTPUT_PARSE_FAILED,
                        format!("failed to parse structured output JSON: {error}"),
                    );
                    None
                }
            }
        }
        ResponseFormat::JsonObject => {
            if let Some(first_text) = text_blocks.first()
                && let Ok(parsed) = serde_json::from_str::<Value>(first_text)
                && parsed.is_object()
            {
                return Some(parsed);
            }

            for text in text_blocks {
                if let Ok(parsed) = serde_json::from_str::<Value>(text)
                    && parsed.is_object()
                {
                    return Some(parsed);
                }
            }

            let combined = text_blocks.join("\n");
            if let Some(object_text) = extract_first_json_object(&combined)
                && let Ok(parsed) = serde_json::from_str::<Value>(&object_text)
                && parsed.is_object()
            {
                return Some(parsed);
            }

            push_warning(
                warnings,
                WARN_STRUCTURED_OUTPUT_PARSE_FAILED,
                "failed to parse json_object structured output from anthropic text blocks",
            );
            None
        }
    }
}

fn decode_usage(
    usage_value: Option<&Value>,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<Usage, AnthropicFamilyError> {
    let Some(usage_value) = usage_value else {
        push_warning(
            warnings,
            WARN_USAGE_MISSING,
            "anthropic response missing usage object",
        );
        return Ok(Usage::default());
    };

    let usage_obj = usage_value
        .as_object()
        .ok_or_else(|| AnthropicFamilyError::decode("anthropic usage must be a JSON object"))?;

    let input_tokens = parse_usage_u64(usage_obj.get("input_tokens"), "input_tokens")?;
    let cache_creation_input_tokens = parse_usage_u64(
        usage_obj.get("cache_creation_input_tokens"),
        "cache_creation_input_tokens",
    )?;
    let cache_read_input_tokens = parse_usage_u64(
        usage_obj.get("cache_read_input_tokens"),
        "cache_read_input_tokens",
    )?;
    let output_tokens = parse_usage_u64(usage_obj.get("output_tokens"), "output_tokens")?;

    if input_tokens.is_none() || output_tokens.is_none() {
        push_warning(
            warnings,
            WARN_USAGE_PARTIAL,
            "anthropic usage object missing required token fields",
        );
    }

    let billed_input = if let Some(base) = input_tokens {
        let with_creation = checked_add_with_warning(
            base,
            cache_creation_input_tokens.unwrap_or(0),
            "input_tokens + cache_creation_input_tokens",
            warnings,
        );
        with_creation.and_then(|sum| {
            checked_add_with_warning(
                sum,
                cache_read_input_tokens.unwrap_or(0),
                "input_tokens + cache_creation_input_tokens + cache_read_input_tokens",
                warnings,
            )
        })
    } else {
        None
    };
    let total_tokens = match (billed_input, output_tokens) {
        (Some(input), Some(output)) => {
            checked_add_with_warning(input, output, "billed_input + output_tokens", warnings)
        }
        _ => None,
    };

    Ok(Usage {
        input_tokens: billed_input,
        output_tokens,
        cached_input_tokens: cache_read_input_tokens,
        total_tokens,
    })
}

fn parse_usage_u64(
    value: Option<&Value>,
    field_name: &str,
) -> Result<Option<u64>, AnthropicFamilyError> {
    match value {
        None => Ok(None),
        Some(Value::Number(number)) => number
            .as_u64()
            .ok_or_else(|| {
                AnthropicFamilyError::decode(format!(
                    "anthropic usage field '{field_name}' must be an unsigned integer",
                ))
            })
            .map(Some),
        Some(_) => Err(AnthropicFamilyError::decode(format!(
            "anthropic usage field '{field_name}' must be numeric",
        ))),
    }
}

fn map_finish_reason(stop_reason: &str, warnings: &mut Vec<RuntimeWarning>) -> FinishReason {
    match stop_reason {
        "end_turn" | "stop_sequence" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        "tool_use" => FinishReason::ToolCalls,
        "refusal" => FinishReason::ContentFilter,
        "pause_turn" => FinishReason::Other,
        other => {
            push_warning(
                warnings,
                WARN_UNKNOWN_STOP_REASON,
                format!("unknown anthropic stop_reason '{other}' mapped to Other"),
            );
            FinishReason::Other
        }
    }
}

fn checked_add_with_warning(
    lhs: u64,
    rhs: u64,
    expression: &str,
    warnings: &mut Vec<RuntimeWarning>,
) -> Option<u64> {
    match lhs.checked_add(rhs) {
        Some(sum) => Some(sum),
        None => {
            push_warning(
                warnings,
                WARN_USAGE_OVERFLOW,
                format!("anthropic usage overflow while computing {expression}"),
            );
            None
        }
    }
}

fn push_warning(warnings: &mut Vec<RuntimeWarning>, code: &str, message: impl Into<String>) {
    warnings.push(RuntimeWarning {
        code: code.to_string(),
        message: message.into(),
    });
}
