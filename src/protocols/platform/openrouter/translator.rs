use serde_json::{Map, Value};

use crate::core::types::{
    AssistantOutput, ContentPart, FinishReason, Request, Response, ResponseFormat, RuntimeWarning,
    ToolCall, Usage,
};
use crate::protocols::error::{AdapterError, AdapterErrorKind, AdapterOperation, AdapterProtocol};
use crate::protocols::openai_spec::decode::decode_openai_response;
use crate::protocols::openai_spec::encode::encode_openai_request;
use crate::protocols::openai_spec::{
    OpenAiDecodeEnvelope, OpenAiEncodedRequest, OpenAiSpecError, OpenAiSpecErrorKind,
};
use crate::protocols::translator_contract::ProtocolTranslator;
use thiserror::Error;

const WARN_IGNORED_TOP_P: &str = "openai.encode.ignored_top_p";
const WARN_IGNORED_STOP: &str = "openai.encode.ignored_stop";
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

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct OpenRouterOverrides {
    pub fallback_models: Vec<String>,
    pub provider_preferences: Option<Value>,
    pub plugins: Vec<Value>,
    pub parallel_tool_calls: Option<bool>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub logit_bias: Option<Value>,
    pub logprobs: Option<bool>,
    pub top_logprobs: Option<u8>,
    pub reasoning: Option<Value>,
    pub seed: Option<i64>,
    pub user: Option<String>,
    pub session_id: Option<String>,
    pub trace: Option<Value>,
    pub route: Option<String>,
    pub max_tokens: Option<u32>,
    pub modalities: Option<Vec<String>>,
    pub image_config: Option<Value>,
    pub debug: Option<Value>,
    pub stream_options: Option<Value>,
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct OpenRouterTranslator {
    overrides: OpenRouterOverrides,
}

impl OpenRouterTranslator {
    pub(crate) fn new(overrides: OpenRouterOverrides) -> Self {
        Self { overrides }
    }
}

impl Default for OpenRouterTranslator {
    fn default() -> Self {
        Self {
            overrides: OpenRouterOverrides::default(),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum OpenRouterTranslatorError {
    #[error("OpenRouter encode error: {0}")]
    Encode(#[source] OpenAiSpecError),
    #[error("OpenRouter decode error: {0}")]
    Decode(#[source] OpenAiSpecError),
}

impl ProtocolTranslator for OpenRouterTranslator {
    type RequestPayload = OpenAiEncodedRequest;
    type ResponsePayload = OpenAiDecodeEnvelope;
    type Error = OpenRouterTranslatorError;

    fn encode_request(&self, req: &Request) -> Result<Self::RequestPayload, Self::Error> {
        let mut encoded = encode_openai_request(req).map_err(OpenRouterTranslatorError::Encode)?;
        apply_openrouter_overrides(
            req,
            &self.overrides,
            &mut encoded.body,
            &mut encoded.warnings,
        )
        .map_err(OpenRouterTranslatorError::Encode)?;
        Ok(encoded)
    }

    fn decode_request(&self, payload: &Self::ResponsePayload) -> Result<Response, Self::Error> {
        match decode_openai_response(payload) {
            Ok(response) => Ok(response),
            Err(openai_decode_error) => {
                if !should_attempt_openrouter_fallback(&openai_decode_error) {
                    return Err(OpenRouterTranslatorError::Decode(openai_decode_error));
                }

                match decode_openrouter_chat_completions_response(payload) {
                    Ok(response) => Ok(response),
                    Err(fallback_error) => Err(OpenRouterTranslatorError::Decode(
                        OpenAiSpecError::decode(format!(
                            "openai-compatible decode failed: {}; openrouter fallback decode failed: {}",
                            openai_decode_error.message(),
                            fallback_error.message()
                        )),
                    )),
                }
            }
        }
    }
}

impl From<OpenRouterTranslatorError> for AdapterError {
    fn from(error: OpenRouterTranslatorError) -> Self {
        let (operation, spec_error) = match &error {
            OpenRouterTranslatorError::Encode(spec_error) => {
                (AdapterOperation::EncodeRequest, spec_error)
            }
            OpenRouterTranslatorError::Decode(spec_error) => {
                (AdapterOperation::DecodeResponse, spec_error)
            }
        };

        AdapterError::with_source(
            map_spec_error_kind(spec_error.kind()),
            AdapterProtocol::OpenRouter,
            operation,
            spec_error.message().to_string(),
            error,
        )
    }
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

fn apply_openrouter_overrides(
    req: &Request,
    overrides: &OpenRouterOverrides,
    request_body: &mut Value,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiSpecError> {
    let body = request_body.as_object_mut().ok_or_else(|| {
        OpenAiSpecError::protocol_violation("encoded request body must be an object")
    })?;

    let mut mapped_top_p = false;
    if let Some(top_p) = req.top_p {
        body.insert("top_p".to_string(), Value::from(top_p));
        mapped_top_p = true;
    }

    let mut mapped_stop = false;
    if !req.stop.is_empty() {
        body.insert(
            "stop".to_string(),
            Value::Array(req.stop.iter().cloned().map(Value::String).collect()),
        );
        mapped_stop = true;
    }

    if mapped_top_p || mapped_stop {
        warnings.retain(|warning| {
            if mapped_top_p && warning.code == WARN_IGNORED_TOP_P {
                return false;
            }
            if mapped_stop && warning.code == WARN_IGNORED_STOP {
                return false;
            }
            true
        });
    }

    if !overrides.fallback_models.is_empty() {
        let mut models = vec![Value::String(req.model_id.clone())];
        for fallback in &overrides.fallback_models {
            if fallback.trim().is_empty() {
                continue;
            }
            models.push(Value::String(fallback.clone()));
        }

        if models.len() > 1 {
            body.remove("model");
            body.insert("models".to_string(), Value::Array(models));
        }
    }

    insert_optional_bool(body, "parallel_tool_calls", overrides.parallel_tool_calls);
    insert_optional_f32(body, "frequency_penalty", overrides.frequency_penalty);
    insert_optional_f32(body, "presence_penalty", overrides.presence_penalty);
    insert_optional_u8(body, "top_logprobs", overrides.top_logprobs);
    insert_optional_i64(body, "seed", overrides.seed);
    insert_optional_u32(body, "max_tokens", overrides.max_tokens);

    if let Some(provider_preferences) = &overrides.provider_preferences {
        body.insert("provider".to_string(), provider_preferences.clone());
    }
    if !overrides.plugins.is_empty() {
        body.insert(
            "plugins".to_string(),
            Value::Array(overrides.plugins.clone()),
        );
    }
    if let Some(logit_bias) = &overrides.logit_bias {
        body.insert("logit_bias".to_string(), logit_bias.clone());
    }
    if let Some(logprobs) = overrides.logprobs {
        body.insert("logprobs".to_string(), Value::Bool(logprobs));
    }
    if let Some(reasoning) = &overrides.reasoning {
        body.insert("reasoning".to_string(), reasoning.clone());
    }
    if let Some(user) = &overrides.user {
        body.insert("user".to_string(), Value::String(user.clone()));
    }
    if let Some(session_id) = &overrides.session_id {
        body.insert("session_id".to_string(), Value::String(session_id.clone()));
    }
    if let Some(trace) = &overrides.trace {
        body.insert("trace".to_string(), trace.clone());
    }
    if let Some(route) = &overrides.route {
        body.insert("route".to_string(), Value::String(route.clone()));
    }
    if let Some(modalities) = &overrides.modalities {
        body.insert(
            "modalities".to_string(),
            Value::Array(modalities.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(image_config) = &overrides.image_config {
        body.insert("image_config".to_string(), image_config.clone());
    }
    if let Some(debug) = &overrides.debug {
        body.insert("debug".to_string(), debug.clone());
    }
    if let Some(stream_options) = &overrides.stream_options {
        body.insert("stream_options".to_string(), stream_options.clone());
    }

    for (key, value) in &overrides.extra {
        body.insert(key.clone(), value.clone());
    }

    Ok(())
}

fn insert_optional_bool(body: &mut Map<String, Value>, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::Bool(value));
    }
}

fn insert_optional_f32(body: &mut Map<String, Value>, key: &str, value: Option<f32>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::from(value));
    }
}

fn insert_optional_u8(body: &mut Map<String, Value>, key: &str, value: Option<u8>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::from(value));
    }
}

fn insert_optional_i64(body: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::from(value));
    }
}

fn insert_optional_u32(body: &mut Map<String, Value>, key: &str, value: Option<u32>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::from(value));
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
                                if let Some(text) = part_obj.get("text").and_then(Value::as_str) {
                                    if !text.is_empty() {
                                        content.push(ContentPart::Text {
                                            text: text.to_string(),
                                        });
                                    }
                                }
                            }
                            "refusal" => {
                                if let Some(text) = part_obj
                                    .get("refusal")
                                    .and_then(Value::as_str)
                                    .or_else(|| part_obj.get("text").and_then(Value::as_str))
                                {
                                    if !text.is_empty() {
                                        content.push(ContentPart::Text {
                                            text: text.to_string(),
                                        });
                                    }
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
    if let Some(text) = value.and_then(Value::as_str) {
        if !text.is_empty() {
            content.push(ContentPart::Text {
                text: text.to_string(),
            });
        }
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

    let calls = value
        .as_array()
        .ok_or_else(|| OpenAiSpecError::decode("openrouter message tool_calls must be an array"))?;

    for (idx, call) in calls.iter().enumerate() {
        let call_obj = call
            .as_object()
            .ok_or_else(|| OpenAiSpecError::decode("openrouter tool_call must be an object"))?;

        let id = match call_obj.get("id").and_then(Value::as_str) {
            Some(id) if !id.trim().is_empty() => id.to_string(),
            _ => {
                warnings.push(RuntimeWarning {
                    code: WARN_MISSING_TOOL_CALL_ID.to_string(),
                    message: format!(
                        "openrouter tool_call at index {idx} missing id; generated synthetic id"
                    ),
                });
                format!("openrouter_tool_call_{idx}")
            }
        };

        let function = call_obj.get("function").and_then(Value::as_object);
        let name = function
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
            .or_else(|| call_obj.get("name").and_then(Value::as_str));

        let Some(name) = name else {
            warnings.push(RuntimeWarning {
                code: WARN_MISSING_TOOL_CALL_NAME.to_string(),
                message: format!(
                    "openrouter tool_call at index {idx} missing function name and was ignored"
                ),
            });
            continue;
        };

        let arguments_value = function
            .and_then(|function| function.get("arguments"))
            .or_else(|| call_obj.get("arguments"));

        let arguments_json = match arguments_value {
            Some(Value::String(arguments)) => match serde_json::from_str::<Value>(arguments) {
                Ok(parsed) => parsed,
                Err(_) => {
                    warnings.push(RuntimeWarning {
                        code: WARN_INVALID_TOOL_CALL_ARGUMENTS.to_string(),
                        message: format!(
                            "openrouter tool_call arguments for '{name}' were not valid JSON; preserved raw string"
                        ),
                    });
                    Value::String(arguments.to_string())
                }
            },
            Some(other) => other.clone(),
            None => Value::Object(Map::new()),
        };

        content.push(ContentPart::ToolCall {
            tool_call: ToolCall {
                id,
                name: name.to_string(),
                arguments_json,
            },
        });
    }

    Ok(())
}

fn decode_openrouter_structured_output(
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
                    warnings.push(RuntimeWarning {
                        code: WARN_STRUCTURED_OUTPUT_NOT_OBJECT.to_string(),
                        message: "structured output is valid JSON but not a JSON object"
                            .to_string(),
                    });
                    None
                }
            }
            ResponseFormat::JsonSchema { .. } => Some(parsed),
            ResponseFormat::Text => None,
        },
        Err(_) => {
            warnings.push(RuntimeWarning {
                code: WARN_STRUCTURED_OUTPUT_PARSE_FAILED.to_string(),
                message: "unable to parse structured output JSON from decoded text".to_string(),
            });
            None
        }
    }
}

fn decode_openrouter_usage(usage: Option<&Value>) -> Usage {
    let Some(usage_obj) = usage.and_then(Value::as_object) else {
        return Usage::default();
    };

    let input_tokens = usage_obj
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .or_else(|| usage_obj.get("input_tokens").and_then(Value::as_u64));
    let output_tokens = usage_obj
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or_else(|| usage_obj.get("output_tokens").and_then(Value::as_u64));
    let total_tokens = usage_obj.get("total_tokens").and_then(Value::as_u64);
    let cached_input_tokens = usage_obj
        .get("prompt_tokens_details")
        .and_then(Value::as_object)
        .and_then(|details| details.get("cached_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| {
            usage_obj
                .get("input_tokens_details")
                .and_then(Value::as_object)
                .and_then(|details| details.get("cached_tokens"))
                .and_then(Value::as_u64)
        });

    Usage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
    }
}

fn map_openrouter_finish_reason(
    finish_reason: Option<&str>,
    content: &[ContentPart],
    warnings: &mut Vec<RuntimeWarning>,
) -> FinishReason {
    match finish_reason {
        Some("stop") => {
            if should_finish_with_tool_calls(content) {
                FinishReason::ToolCalls
            } else {
                FinishReason::Stop
            }
        }
        Some("length") => FinishReason::Length,
        Some("tool_calls") => FinishReason::ToolCalls,
        Some("content_filter") => FinishReason::ContentFilter,
        Some("error") => FinishReason::Error,
        Some(other) => {
            warnings.push(RuntimeWarning {
                code: WARN_UNKNOWN_FINISH_REASON.to_string(),
                message: format!("unrecognized openrouter finish_reason '{other}'"),
            });
            FinishReason::Other
        }
        None => FinishReason::Other,
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
