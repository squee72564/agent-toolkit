use agent_core::{
    OpenAiOptions, OpenAiPromptCacheRetention, OpenAiServiceTier, OpenAiTextOptions,
    OpenAiTextVerbosity, OpenAiTruncation, ProviderKind, ProviderOptions, Response, ResponseFormat,
    TaskRequest,
};
use serde_json::Value;

use crate::{
    error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo},
    families::openai_compatible::wire::decode::parse_openai_error_value,
    interfaces::{ProviderRefinement, ProviderStreamProjector},
    request_plan::EncodedFamilyRequest,
};

#[derive(Debug, Clone, Default, PartialEq)]
struct OpenAiNativeOptionsOverrides {
    metadata: serde_json::Map<String, Value>,
    service_tier: Option<&'static str>,
    store: Option<bool>,
    prompt_cache_key: Option<String>,
    prompt_cache_retention: Option<String>,
    truncation: Option<String>,
    text_verbosity: Option<String>,
    safety_identifier: Option<String>,
    previous_response_id: Option<String>,
    top_logprobs: Option<u32>,
    max_tool_calls: Option<u32>,
}

impl OpenAiNativeOptionsOverrides {
    fn from_options(provider_options: Option<&ProviderOptions>) -> Result<Self, AdapterError> {
        let mut overrides = Self::default();

        if let Some(provider_options) = provider_options {
            match provider_options {
                ProviderOptions::OpenAi(OpenAiOptions {
                    metadata,
                    service_tier,
                    store,
                    prompt_cache_key,
                    prompt_cache_retention,
                    truncation,
                    text,
                    safety_identifier,
                    previous_response_id,
                    top_logprobs,
                    max_tool_calls,
                }) => {
                    overrides.metadata = metadata
                        .iter()
                        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                        .collect();
                    overrides.service_tier = service_tier.as_ref().map(service_tier_name);
                    overrides.store = *store;
                    overrides.prompt_cache_key = prompt_cache_key.clone();
                    overrides.prompt_cache_retention = prompt_cache_retention
                        .as_ref()
                        .map(prompt_cache_retention_name);
                    overrides.truncation = truncation.as_ref().map(truncation_name);
                    overrides.text_verbosity = text
                        .as_ref()
                        .and_then(|OpenAiTextOptions { verbosity }| verbosity.as_ref())
                        .map(text_verbosity_name);
                    overrides.safety_identifier = safety_identifier.clone();
                    overrides.previous_response_id = previous_response_id.clone();
                    overrides.top_logprobs = *top_logprobs;
                    overrides.max_tool_calls = *max_tool_calls;
                }
                other => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Validation,
                        ProviderKind::OpenAi,
                        AdapterOperation::PlanRequest,
                        format!(
                            "OpenAI refinement layer received mismatched provider native options for {:?}",
                            other.provider_kind()
                        ),
                    ));
                }
            }
        }

        overrides.validate_metadata()?;
        overrides.validate_safety_identifier()?;
        overrides.validate_top_logprobs()?;
        overrides.validate_max_tool_calls()?;

        Ok(overrides)
    }

    fn apply(&self, request: &mut EncodedFamilyRequest) -> Result<(), AdapterError> {
        let Some(body) = request.body.as_object_mut() else {
            return Err(AdapterError::new(
                AdapterErrorKind::ProtocolViolation,
                ProviderKind::OpenAi,
                AdapterOperation::PlanRequest,
                "OpenAI family request body must be an object",
            ));
        };

        if !self.metadata.is_empty() {
            body.insert("metadata".to_string(), Value::Object(self.metadata.clone()));
        }
        if let Some(service_tier) = self.service_tier.as_ref() {
            body.insert(
                "service_tier".to_string(),
                Value::String((*service_tier).to_string()),
            );
        }
        if let Some(store) = self.store {
            body.insert("store".to_string(), Value::Bool(store));
        }
        if let Some(prompt_cache_key) = self.prompt_cache_key.as_ref() {
            body.insert(
                "prompt_cache_key".to_string(),
                Value::String(prompt_cache_key.clone()),
            );
        }
        if let Some(prompt_cache_retention) = self.prompt_cache_retention.as_ref() {
            body.insert(
                "prompt_cache_retention".to_string(),
                Value::String(prompt_cache_retention.clone()),
            );
        }
        if let Some(truncation) = self.truncation.as_ref() {
            body.insert("truncation".to_string(), Value::String(truncation.clone()));
        }
        if let Some(safety_identifier) = self.safety_identifier.as_ref() {
            body.insert(
                "safety_identifier".to_string(),
                Value::String(safety_identifier.clone()),
            );
        }
        if let Some(previous_response_id) = self.previous_response_id.as_ref() {
            body.insert(
                "previous_response_id".to_string(),
                Value::String(previous_response_id.clone()),
            );
        }
        if let Some(top_logprobs) = self.top_logprobs {
            body.insert(
                "top_logprobs".to_string(),
                Value::Number(top_logprobs.into()),
            );
        }
        if let Some(max_tool_calls) = self.max_tool_calls {
            body.insert(
                "max_tool_calls".to_string(),
                Value::Number(max_tool_calls.into()),
            );
        }
        if let Some(verbosity) = self.text_verbosity.as_ref() {
            let Some(text) = body.get_mut("text").and_then(Value::as_object_mut) else {
                return Err(AdapterError::new(
                    AdapterErrorKind::ProtocolViolation,
                    ProviderKind::OpenAi,
                    AdapterOperation::PlanRequest,
                    "OpenAI family request body must contain an object text field",
                ));
            };
            text.insert("verbosity".to_string(), Value::String(verbosity.clone()));
        }

        Ok(())
    }

    fn validate_metadata(&self) -> Result<(), AdapterError> {
        if self.metadata.len() > 16 {
            return Err(AdapterError::new(
                AdapterErrorKind::Validation,
                ProviderKind::OpenAi,
                AdapterOperation::PlanRequest,
                "OpenAI metadata must contain at most 16 pairs",
            ));
        }

        for (key, value) in &self.metadata {
            if key.len() > 64 {
                return Err(AdapterError::new(
                    AdapterErrorKind::Validation,
                    ProviderKind::OpenAi,
                    AdapterOperation::PlanRequest,
                    "OpenAI metadata keys must be at most 64 characters",
                ));
            }

            let Some(value) = value.as_str() else {
                return Err(AdapterError::new(
                    AdapterErrorKind::ProtocolViolation,
                    ProviderKind::OpenAi,
                    AdapterOperation::PlanRequest,
                    "OpenAI metadata overrides must serialize to string values",
                ));
            };

            if value.len() > 512 {
                return Err(AdapterError::new(
                    AdapterErrorKind::Validation,
                    ProviderKind::OpenAi,
                    AdapterOperation::PlanRequest,
                    "OpenAI metadata values must be at most 512 characters",
                ));
            }
        }

        Ok(())
    }

    fn validate_safety_identifier(&self) -> Result<(), AdapterError> {
        if self.safety_identifier.as_ref().map_or(0, String::len) > 64 {
            return Err(AdapterError::new(
                AdapterErrorKind::Validation,
                ProviderKind::OpenAi,
                AdapterOperation::PlanRequest,
                "OpenAI safety_identifier must be at most 64 characters",
            ));
        }

        Ok(())
    }

    fn validate_top_logprobs(&self) -> Result<(), AdapterError> {
        if self
            .top_logprobs
            .is_some_and(|top_logprobs| top_logprobs > 20)
        {
            return Err(AdapterError::new(
                AdapterErrorKind::Validation,
                ProviderKind::OpenAi,
                AdapterOperation::PlanRequest,
                "OpenAI top_logprobs must be in range 0..=20",
            ));
        }

        Ok(())
    }

    fn validate_max_tool_calls(&self) -> Result<(), AdapterError> {
        if self.max_tool_calls == Some(0) {
            return Err(AdapterError::new(
                AdapterErrorKind::Validation,
                ProviderKind::OpenAi,
                AdapterOperation::PlanRequest,
                "OpenAI max_tool_calls must be greater than 0",
            ));
        }

        Ok(())
    }
}

fn service_tier_name(value: &OpenAiServiceTier) -> &'static str {
    match value {
        OpenAiServiceTier::Auto => "auto",
        OpenAiServiceTier::Default => "default",
        OpenAiServiceTier::Flex => "flex",
        OpenAiServiceTier::Scale => "scale",
        OpenAiServiceTier::Priority => "priority",
    }
}

fn prompt_cache_retention_name(value: &OpenAiPromptCacheRetention) -> String {
    match value {
        OpenAiPromptCacheRetention::InMemory => "in-memory".to_string(),
        OpenAiPromptCacheRetention::TwentyFourHours => "24h".to_string(),
    }
}

fn truncation_name(value: &OpenAiTruncation) -> String {
    match value {
        OpenAiTruncation::Auto => "auto".to_string(),
        OpenAiTruncation::Disabled => "disabled".to_string(),
    }
}

fn text_verbosity_name(value: &OpenAiTextVerbosity) -> String {
    match value {
        OpenAiTextVerbosity::Low => "low".to_string(),
        OpenAiTextVerbosity::Medium => "medium".to_string(),
        OpenAiTextVerbosity::High => "high".to_string(),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenAiRefinement;

impl ProviderRefinement for OpenAiRefinement {
    fn refine_request(
        &self,
        _task: &TaskRequest,
        _model: &str,
        request: &mut EncodedFamilyRequest,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<(), AdapterError> {
        OpenAiNativeOptionsOverrides::from_options(provider_options)?.apply(request)
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
        None
    }
}
