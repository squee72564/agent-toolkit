use serde::Serialize;
use serde_json::{Map, Value};

use agent_core::{
    OpenRouterOptions, ProviderKind, ProviderOptions, Response, ResponseFormat, RuntimeWarning,
    TaskRequest,
};

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo};
use crate::openai_family::OpenAiFamilyError;
use crate::request_plan::EncodedFamilyRequest;
use crate::stream_projector::ProviderStreamProjector;

use crate::refinement::ProviderRefinement;
use crate::refinement::openrouter_stream_projector::OpenRouterStreamProjector;

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
    fn from_options(provider_options: Option<&ProviderOptions>) -> Result<Self, AdapterError> {
        let mut overrides = Self::default();

        if let Some(provider) = provider_options {
            let ProviderOptions::OpenRouter(options) = provider else {
                return Err(AdapterError::new(
                    AdapterErrorKind::Validation,
                    ProviderKind::OpenRouter,
                    AdapterOperation::PlanRequest,
                    format!(
                        "OpenRouter refinement received mismatched provider native options for {:?}",
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenRouterOverlay;

impl ProviderRefinement for OpenRouterOverlay {
    fn refine_request(
        &self,
        task: &TaskRequest,
        model: &str,
        request: &mut EncodedFamilyRequest,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<(), AdapterError> {
        let overrides = OpenRouterOverrides::from_options(provider_options)?;
        apply_openrouter_overrides(
            model,
            task.top_p,
            &task.stop,
            &overrides,
            &mut request.body,
            &mut request.warnings,
        )
        .map_err(map_openrouter_plan_error)
    }

    fn decode_provider_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        let envelope = crate::openai_family::decode::parse_openai_error_value(body)?;
        Some(ProviderErrorInfo {
            provider_code: envelope.code.or(envelope.error_type),
            message: None,
            kind: None,
        })
    }

    fn decode_response_override(
        &self,
        _body: Value,
        _requested_format: &ResponseFormat,
    ) -> Option<Result<Response, AdapterError>> {
        None
    }

    fn create_stream_projector_override(&self) -> Option<Box<dyn ProviderStreamProjector>> {
        Some(Box::<OpenRouterStreamProjector>::default())
    }
}

pub(crate) fn apply_openrouter_overrides(
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
        let number = serde_json::Number::from_f64(f64::from(value)).ok_or_else(|| {
            OpenAiFamilyError::validation(format!("{key} must be finite for OpenRouter"))
        })?;
        body.insert(key.to_string(), Value::Number(number));
    }
    Ok(())
}

fn map_openrouter_plan_error(error: OpenAiFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        match error.kind() {
            crate::openai_family::OpenAiFamilyErrorKind::Validation => AdapterErrorKind::Validation,
            crate::openai_family::OpenAiFamilyErrorKind::Encode => AdapterErrorKind::Encode,
            crate::openai_family::OpenAiFamilyErrorKind::Decode => AdapterErrorKind::Decode,
            crate::openai_family::OpenAiFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
            crate::openai_family::OpenAiFamilyErrorKind::ProtocolViolation => {
                AdapterErrorKind::ProtocolViolation
            }
            crate::openai_family::OpenAiFamilyErrorKind::UnsupportedFeature => {
                AdapterErrorKind::UnsupportedFeature
            }
        },
        ProviderKind::OpenRouter,
        AdapterOperation::PlanRequest,
        message,
        error,
    )
}
