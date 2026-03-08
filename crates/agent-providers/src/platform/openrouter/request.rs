use serde_json::{Map, Value};

use agent_core::{Request, RuntimeWarning};
use agent_transport::HttpRequestOptions;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_spec::encode::{OpenAiEncodeInput, encode_openai_request_parts};
use crate::openai_spec::{OpenAiSpecError, OpenAiSpecErrorKind};
use crate::request_plan::{ProviderRequestPlan, ProviderResponseKind, ProviderTransportKind};

const WARN_IGNORED_TOP_P: &str = "openai.encode.ignored_top_p";
const WARN_IGNORED_STOP: &str = "openai.encode.ignored_stop";

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

pub(crate) fn plan_request(
    req: Request,
    overrides: &OpenRouterOverrides,
) -> Result<ProviderRequestPlan, AdapterError> {
    let Request {
        model_id,
        stream,
        messages,
        tools,
        tool_choice,
        response_format,
        temperature,
        top_p,
        max_output_tokens,
        stop,
        metadata,
    } = req;

    let mut encoded = encode_openai_request_parts(OpenAiEncodeInput {
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
    .map_err(map_openrouter_plan_error)?;
    apply_openrouter_overrides(
        &model_id,
        top_p,
        &stop,
        overrides,
        &mut encoded.body,
        &mut encoded.warnings,
    )
    .map_err(map_openrouter_plan_error)?;
    if stream {
        encoded.body["stream"] = Value::Bool(true);
    }

    Ok(ProviderRequestPlan {
        body: encoded.body,
        warnings: encoded.warnings,
        transport_kind: if stream {
            ProviderTransportKind::HttpSse
        } else {
            ProviderTransportKind::HttpJson
        },
        response_kind: if stream {
            ProviderResponseKind::RawProviderStream
        } else {
            ProviderResponseKind::JsonBody
        },
        endpoint_path_override: None,
        request_options: if stream {
            HttpRequestOptions::sse_defaults()
        } else {
            HttpRequestOptions::json_defaults().with_allow_error_status(true)
        },
    })
}

fn apply_openrouter_overrides(
    model_id: &str,
    top_p: Option<f32>,
    stop: &[String],
    overrides: &OpenRouterOverrides,
    request_body: &mut Value,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiSpecError> {
    let body = request_body.as_object_mut().ok_or_else(|| {
        OpenAiSpecError::protocol_violation("encoded request body must be an object")
    })?;

    let mut mapped_top_p = false;
    if let Some(top_p) = top_p {
        body.insert("top_p".to_string(), Value::from(top_p));
        mapped_top_p = true;
    }

    let mut mapped_stop = false;
    if !stop.is_empty() {
        body.insert(
            "stop".to_string(),
            Value::Array(stop.iter().cloned().map(Value::String).collect()),
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
        let mut models = vec![Value::String(model_id.to_string())];
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
    insert_optional_f32(body, "frequency_penalty", overrides.frequency_penalty)?;
    insert_optional_f32(body, "presence_penalty", overrides.presence_penalty)?;
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

fn insert_optional_u8(body: &mut Map<String, Value>, key: &str, value: Option<u8>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::from(value));
    }
}

fn insert_optional_u32(body: &mut Map<String, Value>, key: &str, value: Option<u32>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::from(value));
    }
}

fn insert_optional_i64(body: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::from(value));
    }
}

fn insert_optional_f32(
    body: &mut Map<String, Value>,
    key: &str,
    value: Option<f32>,
) -> Result<(), OpenAiSpecError> {
    if let Some(value) = value {
        let number = serde_json::Number::from_f64(f64::from(value))
            .ok_or_else(|| OpenAiSpecError::validation(format!("{key} must be finite")))?;
        body.insert(key.to_string(), Value::Number(number));
    }
    Ok(())
}

fn map_openrouter_plan_error(error: OpenAiSpecError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::OpenRouter,
        AdapterOperation::PlanRequest,
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
