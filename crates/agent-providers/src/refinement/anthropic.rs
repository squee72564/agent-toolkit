use agent_core::{
    AnthropicOptions, ProviderKind, ProviderOptions, Response, ResponseFormat, TaskRequest,
};
use serde_json::Value;

use crate::anthropic_family::decode::parse_anthropic_error_value;
use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo};
use crate::interfaces::ProviderRefinement;
use crate::interfaces::ProviderStreamProjector;
use crate::request_plan::EncodedFamilyRequest;

#[derive(Debug, Clone, Default, PartialEq)]
struct AnthropicNativeOptionsOverrides {
    top_k: Option<u32>,
}

impl AnthropicNativeOptionsOverrides {
    fn from_options(provider_options: Option<&ProviderOptions>) -> Result<Self, AdapterError> {
        let mut overrides = Self::default();

        if let Some(provider_options) = provider_options {
            let ProviderOptions::Anthropic(AnthropicOptions { top_k }) = provider_options else {
                return Err(AdapterError::new(
                    AdapterErrorKind::Validation,
                    ProviderKind::Anthropic,
                    AdapterOperation::PlanRequest,
                    format!(
                        "Anthropic refinement layer received mismatched provider native options for {:?}",
                        provider_options.provider_kind()
                    ),
                ));
            };
            overrides.top_k = *top_k;
        }

        Ok(overrides)
    }

    fn apply(&self, request: &mut EncodedFamilyRequest) -> Result<(), AdapterError> {
        let Some(body) = request.body.as_object_mut() else {
            return Err(AdapterError::new(
                AdapterErrorKind::ProtocolViolation,
                ProviderKind::Anthropic,
                AdapterOperation::PlanRequest,
                "Anthropic family request body must be an object",
            ));
        };

        if let Some(top_k) = self.top_k {
            body.insert("top_k".to_string(), Value::from(top_k));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnthropicOverlay;

impl ProviderRefinement for AnthropicOverlay {
    fn refine_request(
        &self,
        _task: &TaskRequest,
        _model: &str,
        request: &mut EncodedFamilyRequest,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<(), AdapterError> {
        AnthropicNativeOptionsOverrides::from_options(provider_options)?.apply(request)
    }

    fn decode_provider_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        let root = body.as_object()?;
        let envelope = parse_anthropic_error_value(root)?;
        Some(ProviderErrorInfo {
            provider_code: envelope.error_type,
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
