use serde::Serialize;
use serde_json::{Map, Value};

use agent_core::{
    FamilyOptions, NativeOptions, OpenRouterOptions, ProviderOptions, Request, RuntimeWarning,
};
use agent_transport::HttpRequestOptions;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_family::encode::{OpenAiEncodeInput, encode_openai_request_parts};
use crate::openai_family::{OpenAiFamilyError, OpenAiFamilyErrorKind};
use crate::request_plan::{ProviderRequestPlan, ProviderResponseKind, ProviderTransportKind};

const WARN_IGNORED_TOP_P: &str = "openai.encode.ignored_top_p";
const WARN_IGNORED_STOP: &str = "openai.encode.ignored_stop";

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub(crate) struct OpenRouterOverrides {
    #[serde(skip_serializing)]
    pub fallback_models: Vec<String>,
    #[serde(rename = "provider", skip_serializing_if = "Option::is_none")]
    pub provider_preferences: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing)]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing)]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<Value>,
    #[serde(skip_serializing)]
    pub extra: Map<String, Value>,
}

impl OpenRouterOverrides {
    pub(crate) fn from_native_options(
        native_options: Option<&NativeOptions>,
    ) -> Result<Self, AdapterError> {
        let Some(native_options) = native_options else {
            return Ok(Self::default());
        };

        let mut overrides = Self::default();

        if let Some(FamilyOptions::OpenAiCompatible(options)) = native_options.family.as_ref() {
            overrides.parallel_tool_calls = options.parallel_tool_calls;
            overrides.reasoning = options.reasoning.clone();
        }

        if let Some(provider) = native_options.provider.as_ref() {
            let ProviderOptions::OpenRouter(options) = provider else {
                return Err(AdapterError::new(
                    AdapterErrorKind::Validation,
                    agent_core::ProviderId::OpenRouter,
                    AdapterOperation::PlanRequest,
                    format!(
                        "OpenRouter adapter received mismatched provider native options for {:?}",
                        provider.provider_kind()
                    ),
                ));
            };
            apply_provider_options(&mut overrides, options);
        }

        Ok(overrides)
    }
}

fn apply_provider_options(overrides: &mut OpenRouterOverrides, options: &OpenRouterOptions) {
    overrides.fallback_models = options.fallback_models.clone();
    overrides.provider_preferences = options.provider_preferences.clone();
    overrides.plugins = options.plugins.clone();
    overrides.frequency_penalty = options.frequency_penalty;
    overrides.presence_penalty = options.presence_penalty;
    overrides.logit_bias = options.logit_bias.clone();
    overrides.logprobs = options.logprobs;
    overrides.top_logprobs = options.top_logprobs;
    overrides.seed = options.seed;
    overrides.user = options.user.clone();
    overrides.session_id = options.session_id.clone();
    overrides.trace = options.trace.clone();
    overrides.route = options.route.clone();
    overrides.modalities = options.modalities.clone();
    overrides.image_config = options.image_config.clone();
    overrides.debug = options.debug.clone();
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
) -> Result<(), OpenAiFamilyError> {
    let body = request_body.as_object_mut().ok_or_else(|| {
        OpenAiFamilyError::protocol_violation("encoded request body must be an object")
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

    insert_optional_f32(body, "frequency_penalty", overrides.frequency_penalty)?;
    insert_optional_f32(body, "presence_penalty", overrides.presence_penalty)?;
    let serialized_overrides = serde_json::to_value(overrides).map_err(|error| {
        OpenAiFamilyError::protocol_violation(format!(
            "failed to serialize OpenRouter overrides: {error}"
        ))
    })?;
    let serialized_overrides = serialized_overrides.as_object().ok_or_else(|| {
        OpenAiFamilyError::protocol_violation("serialized OpenRouter overrides must be an object")
    })?;
    for (key, value) in serialized_overrides {
        body.insert(key.clone(), value.clone());
    }

    for (key, value) in &overrides.extra {
        body.insert(key.clone(), value.clone());
    }

    Ok(())
}

fn insert_optional_f32(
    body: &mut Map<String, Value>,
    key: &str,
    value: Option<f32>,
) -> Result<(), OpenAiFamilyError> {
    if let Some(value) = value {
        let number = serde_json::Number::from_f64(f64::from(value))
            .ok_or_else(|| OpenAiFamilyError::validation(format!("{key} must be finite")))?;
        body.insert(key.to_string(), Value::Number(number));
    }
    Ok(())
}

fn map_openrouter_plan_error(error: OpenAiFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::OpenRouter,
        AdapterOperation::PlanRequest,
        message,
        error,
    )
}

fn map_spec_error_kind(kind: OpenAiFamilyErrorKind) -> AdapterErrorKind {
    match kind {
        OpenAiFamilyErrorKind::Validation => AdapterErrorKind::Validation,
        OpenAiFamilyErrorKind::Encode => AdapterErrorKind::Encode,
        OpenAiFamilyErrorKind::Decode => AdapterErrorKind::Decode,
        OpenAiFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
        OpenAiFamilyErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        OpenAiFamilyErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}
