use serde_json::{Map, Value};

use agent_core::{
    OpenRouterOptions, OpenRouterTextVerbosity, ProviderKind, ProviderOptions, Response,
    ResponseFormat, RuntimeWarning, TaskRequest,
};

use crate::{
    error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo},
    families::openai_compatible::wire::{
        OpenAiFamilyError, OpenAiFamilyErrorKind, decode::parse_openai_error_value,
    },
    interfaces::{ProviderRefinement, ProviderStreamProjector},
    providers::openrouter::stream_projector::OpenRouterStreamProjector,
    request_plan::EncodedFamilyRequest,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct OpenRouterOverrides {
    pub fallback_models: Vec<String>,
    pub provider_preferences: Option<Value>,
    pub plugins: Vec<Value>,
    pub metadata: Map<String, Value>,
    pub top_k: Option<u32>,
    pub top_logprobs: Option<u8>,
    pub max_tokens: Option<u32>,
    pub stop: Vec<String>,
    pub seed: Option<i64>,
    pub logit_bias: Map<String, Value>,
    pub logprobs: Option<bool>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub user: Option<String>,
    pub session_id: Option<String>,
    pub trace: Option<Value>,
    pub text_verbosity: Option<OpenRouterTextVerbosity>,
    pub modalities: Option<Vec<String>>,
    pub image_config: Option<Value>,
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
    overrides.metadata = options
        .metadata
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect();
    overrides.top_k = options.top_k;
    overrides.top_logprobs = options.top_logprobs;
    overrides.max_tokens = options.max_tokens;
    overrides.stop = options.stop.clone();
    overrides.seed = options.seed;
    overrides.logit_bias = options
        .logit_bias
        .iter()
        .map(|(token_id, bias)| (token_id.clone(), Value::from(*bias)))
        .collect();
    overrides.logprobs = options.logprobs;
    overrides.frequency_penalty = options.frequency_penalty;
    overrides.presence_penalty = options.presence_penalty;
    overrides.user = options.user.clone();
    overrides.session_id = options.session_id.clone();
    overrides.trace = options.trace.clone();
    overrides.text_verbosity = options.text.as_ref().and_then(|text| text.verbosity);
    overrides.modalities = options.modalities.clone();
    overrides.image_config = options.image_config.clone();
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenRouterRefinement;

impl ProviderRefinement for OpenRouterRefinement {
    fn refine_request(
        &self,
        _task: &TaskRequest,
        model: &str,
        request: &mut EncodedFamilyRequest,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<(), AdapterError> {
        let overrides = OpenRouterOverrides::from_options(provider_options)?;
        apply_openrouter_overrides(model, &overrides, &mut request.body, &mut request.warnings)
            .map_err(map_openrouter_plan_error)
    }

    fn decode_provider_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        let envelope = parse_openai_error_value(body)?;
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
    overrides: &OpenRouterOverrides,
    request_body: &mut Value,
    warnings: &mut Vec<RuntimeWarning>,
) -> Result<(), OpenAiFamilyError> {
    let body = request_body.as_object_mut().ok_or_else(|| {
        OpenAiFamilyError::protocol_violation("encoded request body must be an object")
    })?;

    let _ = warnings;

    validate_metadata(&overrides.metadata)?;
    validate_optional_range(overrides.frequency_penalty, "frequency_penalty", -2.0, 2.0)?;
    validate_optional_range(overrides.presence_penalty, "presence_penalty", -2.0, 2.0)?;
    validate_optional_length(&overrides.user, "user", 128)?;
    validate_optional_length(&overrides.session_id, "session_id", 128)?;
    validate_modalities(overrides.modalities.as_ref())?;
    validate_top_logprobs(overrides.top_logprobs, overrides.logprobs)?;
    validate_max_tokens(overrides.max_tokens, body.get("max_output_tokens"))?;
    validate_logit_bias(&overrides.logit_bias)?;

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

    if let Some(provider_preferences) = overrides.provider_preferences.as_ref() {
        body.insert("provider".to_string(), provider_preferences.clone());
    }
    if !overrides.plugins.is_empty() {
        body.insert(
            "plugins".to_string(),
            Value::Array(overrides.plugins.clone()),
        );
    }
    if !overrides.metadata.is_empty() {
        body.insert(
            "metadata".to_string(),
            Value::Object(overrides.metadata.clone()),
        );
    }
    insert_optional_u32(body, "top_k", overrides.top_k);
    insert_optional_u8(body, "top_logprobs", overrides.top_logprobs);
    insert_optional_u32(body, "max_tokens", overrides.max_tokens);
    if !overrides.stop.is_empty() {
        body.insert(
            "stop".to_string(),
            Value::Array(overrides.stop.iter().cloned().map(Value::String).collect()),
        );
    }
    insert_optional_i64(body, "seed", overrides.seed);
    if !overrides.logit_bias.is_empty() {
        body.insert(
            "logit_bias".to_string(),
            Value::Object(overrides.logit_bias.clone()),
        );
    }
    insert_optional_bool(body, "logprobs", overrides.logprobs);
    insert_optional_f32(body, "frequency_penalty", overrides.frequency_penalty)?;
    insert_optional_f32(body, "presence_penalty", overrides.presence_penalty)?;
    if let Some(user) = overrides.user.as_ref() {
        body.insert("user".to_string(), Value::String(user.clone()));
    }
    if let Some(session_id) = overrides.session_id.as_ref() {
        body.insert("session_id".to_string(), Value::String(session_id.clone()));
    }
    if let Some(trace) = overrides.trace.as_ref() {
        body.insert("trace".to_string(), trace.clone());
    }
    merge_text_verbosity(body, overrides.text_verbosity)?;
    if let Some(modalities) = overrides.modalities.as_ref() {
        body.insert(
            "modalities".to_string(),
            Value::Array(modalities.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(image_config) = overrides.image_config.as_ref() {
        body.insert("image_config".to_string(), image_config.clone());
    }

    Ok(())
}

fn validate_metadata(metadata: &Map<String, Value>) -> Result<(), OpenAiFamilyError> {
    if metadata.len() > 16 {
        return Err(OpenAiFamilyError::validation(
            "OpenRouter metadata must contain at most 16 entries",
        ));
    }

    for (key, value) in metadata {
        if key.len() > 64 {
            return Err(OpenAiFamilyError::validation(
                "OpenRouter metadata keys must be at most 64 characters",
            ));
        }
        if key.contains('[') || key.contains(']') {
            return Err(OpenAiFamilyError::validation(
                "OpenRouter metadata keys must not contain brackets",
            ));
        }
        let Value::String(value) = value else {
            return Err(OpenAiFamilyError::protocol_violation(
                "OpenRouter metadata values must serialize as strings",
            ));
        };
        if value.len() > 512 {
            return Err(OpenAiFamilyError::validation(
                "OpenRouter metadata values must be at most 512 characters",
            ));
        }
    }

    Ok(())
}

fn validate_optional_range(
    value: Option<f32>,
    field_name: &str,
    min: f32,
    max: f32,
) -> Result<(), OpenAiFamilyError> {
    let Some(value) = value else {
        return Ok(());
    };

    if !value.is_finite() {
        return Err(OpenAiFamilyError::validation(format!(
            "{field_name} must be finite for OpenRouter"
        )));
    }

    if !(min..=max).contains(&value) {
        return Err(OpenAiFamilyError::validation(format!(
            "{field_name} must be in {min}..={max} for OpenRouter"
        )));
    }

    Ok(())
}

fn validate_optional_length(
    value: &Option<String>,
    field_name: &str,
    max_len: usize,
) -> Result<(), OpenAiFamilyError> {
    if let Some(value) = value
        && value.len() > max_len
    {
        return Err(OpenAiFamilyError::validation(format!(
            "{field_name} must be at most {max_len} characters for OpenRouter"
        )));
    }

    Ok(())
}

fn validate_modalities(modalities: Option<&Vec<String>>) -> Result<(), OpenAiFamilyError> {
    let Some(modalities) = modalities else {
        return Ok(());
    };

    for modality in modalities {
        if modality != "text" && modality != "image" {
            return Err(OpenAiFamilyError::validation(format!(
                "OpenRouter modalities entries must be 'text' or 'image', got '{modality}'"
            )));
        }
    }

    Ok(())
}

fn validate_top_logprobs(
    top_logprobs: Option<u8>,
    logprobs: Option<bool>,
) -> Result<(), OpenAiFamilyError> {
    let Some(top_logprobs) = top_logprobs else {
        return Ok(());
    };

    if top_logprobs > 20 {
        return Err(OpenAiFamilyError::validation(
            "OpenRouter top_logprobs must be in 0..=20",
        ));
    }
    if logprobs != Some(true) {
        return Err(OpenAiFamilyError::validation(
            "OpenRouter top_logprobs requires logprobs=true",
        ));
    }

    Ok(())
}

fn validate_max_tokens(
    max_tokens: Option<u32>,
    max_output_tokens: Option<&Value>,
) -> Result<(), OpenAiFamilyError> {
    if max_tokens == Some(0) {
        return Err(OpenAiFamilyError::validation(
            "OpenRouter max_tokens must be greater than 0",
        ));
    }
    if max_tokens.is_some() && max_output_tokens.is_some() {
        return Err(OpenAiFamilyError::validation(
            "OpenRouter max_tokens cannot be combined with family max_output_tokens",
        ));
    }

    Ok(())
}

fn validate_logit_bias(logit_bias: &Map<String, Value>) -> Result<(), OpenAiFamilyError> {
    for value in logit_bias.values() {
        let Some(bias) = value.as_i64() else {
            return Err(OpenAiFamilyError::protocol_violation(
                "OpenRouter logit_bias values must serialize as integers",
            ));
        };
        if !(-100..=100).contains(&bias) {
            return Err(OpenAiFamilyError::validation(
                "OpenRouter logit_bias values must be in -100..=100",
            ));
        }
    }

    Ok(())
}

fn insert_optional_u32(body: &mut Map<String, Value>, key: &str, value: Option<u32>) {
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

fn insert_optional_bool(body: &mut Map<String, Value>, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::Bool(value));
    }
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

fn merge_text_verbosity(
    body: &mut Map<String, Value>,
    verbosity: Option<OpenRouterTextVerbosity>,
) -> Result<(), OpenAiFamilyError> {
    let Some(verbosity) = verbosity else {
        return Ok(());
    };

    let text = body
        .entry("text".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let text = text.as_object_mut().ok_or_else(|| {
        OpenAiFamilyError::protocol_violation("OpenRouter text payload must be an object")
    })?;
    let value = match verbosity {
        OpenRouterTextVerbosity::Low => "low",
        OpenRouterTextVerbosity::Medium => "medium",
        OpenRouterTextVerbosity::High => "high",
        OpenRouterTextVerbosity::Max => "max",
    };
    text.insert("verbosity".to_string(), Value::String(value.to_string()));

    Ok(())
}

fn map_openrouter_plan_error(error: OpenAiFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        match error.kind() {
            OpenAiFamilyErrorKind::Validation => AdapterErrorKind::Validation,
            OpenAiFamilyErrorKind::Encode => AdapterErrorKind::Encode,
            OpenAiFamilyErrorKind::Decode => AdapterErrorKind::Decode,
            OpenAiFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
            OpenAiFamilyErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
            OpenAiFamilyErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
        },
        ProviderKind::OpenRouter,
        AdapterOperation::PlanRequest,
        message,
        error,
    )
}
