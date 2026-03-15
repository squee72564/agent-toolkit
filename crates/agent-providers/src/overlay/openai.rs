use agent_core::{
    OpenAiOptions, ProviderKind, ProviderOptions, Response, ResponseFormat, TaskRequest,
};
use serde_json::Value;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo};
use crate::overlay::ProviderOverlay;
use crate::request_plan::EncodedFamilyRequest;
use crate::stream_projector::ProviderStreamProjector;

#[derive(Debug, Clone, Default, PartialEq)]
struct OpenAiNativeOptionsOverrides {
    service_tier: Option<String>,
    store: Option<bool>,
}

impl OpenAiNativeOptionsOverrides {
    fn from_options(provider_options: Option<&ProviderOptions>) -> Result<Self, AdapterError> {
        let mut overrides = Self::default();

        if let Some(provider_options) = provider_options {
            match provider_options {
                ProviderOptions::OpenAi(OpenAiOptions {
                    service_tier,
                    store,
                }) => {
                    overrides.service_tier = service_tier.clone();
                    overrides.store = *store;
                }
                other => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Validation,
                        ProviderKind::OpenAi,
                        AdapterOperation::PlanRequest,
                        format!(
                            "OpenAI overlay received mismatched provider native options for {:?}",
                            other.provider_kind()
                        ),
                    ));
                }
            }
        }

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

        if let Some(service_tier) = self.service_tier.as_ref() {
            body.insert(
                "service_tier".to_string(),
                Value::String(service_tier.clone()),
            );
        }
        if let Some(store) = self.store {
            body.insert("store".to_string(), Value::Bool(store));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenAiOverlay;

impl ProviderOverlay for OpenAiOverlay {
    fn apply_provider_overlay(
        &self,
        _task: &TaskRequest,
        _model: &str,
        request: &mut EncodedFamilyRequest,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<(), AdapterError> {
        OpenAiNativeOptionsOverrides::from_options(provider_options)?.apply(request)
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
        None
    }
}
