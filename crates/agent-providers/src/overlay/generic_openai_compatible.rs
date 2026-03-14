use agent_core::{ProviderKind, ProviderOptions, Response, ResponseFormat, TaskRequest};
use serde_json::Value;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo};
use crate::request_plan::EncodedFamilyRequest;
use crate::streaming::ProviderStreamProjector;

use super::ProviderOverlay;

#[derive(Debug, Clone, Copy)]
pub(crate) struct GenericOpenAiCompatibleOverlay;

impl ProviderOverlay for GenericOpenAiCompatibleOverlay {
    fn apply_provider_overlay(
        &self,
        _task: &TaskRequest,
        _model: &str,
        _request: &mut EncodedFamilyRequest,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<(), AdapterError> {
        if provider_options.is_some() {
            return Err(AdapterError::new(
                AdapterErrorKind::Validation,
                ProviderKind::GenericOpenAiCompatible,
                AdapterOperation::PlanRequest,
                "generic OpenAI-compatible adapter does not support provider-scoped native options",
            ));
        }

        Ok(())
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
