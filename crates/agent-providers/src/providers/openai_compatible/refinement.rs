use agent_core::{ProviderKind, ProviderOptions, Response, ResponseFormat, TaskRequest};
use serde_json::Value;

use crate::{
    error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo},
    families::openai_compatible::wire::decode::parse_openai_error_value,
    interfaces::{ProviderRefinement, ProviderStreamProjector},
    request_plan::EncodedFamilyRequest,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct GenericOpenAiCompatibleRefinement;

impl ProviderRefinement for GenericOpenAiCompatibleRefinement {
    fn refine_request(
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
