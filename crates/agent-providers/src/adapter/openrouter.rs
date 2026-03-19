use agent_core::{
    AuthStyle, ExecutionPlan, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId,
    ProviderKind, Response, ResponseFormat,
};
use reqwest::header::{HeaderMap, HeaderName};
use serde_json::Value;

use crate::{
    error::{AdapterError, ProviderErrorInfo},
    interfaces::{
        ProviderAdapter, ProviderStreamProjector, codec_for, refinement_for,
    },
    adapter::{
        create_stream_projector_with_layering,
        decode_error_with_layering, decode_response_with_layering, plan_request_with_layering,
        rebind_adapter_error_provider,
    },
    request_plan::ProviderRequestPlan,
};

pub(crate) const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api";
pub(crate) const OPENROUTER_ENDPOINT_PATH: &str = "/v1/responses";

#[derive(Debug, Clone, Copy)]
/// Built-in adapter for OpenRouter.
///
/// OpenRouter reuses the OpenAI-compatible family codec and adds
/// provider-specific request/stream refinements on top.
pub struct OpenRouterAdapter;

pub(crate) fn openrouter_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        kind: ProviderKind::OpenRouter,
        family: ProviderFamilyId::OpenAiCompatible,
        protocol: agent_core::ProtocolKind::OpenAI,
        default_base_url: OPENROUTER_BASE_URL,
        endpoint_path: OPENROUTER_ENDPOINT_PATH,
        default_auth_style: AuthStyle::Bearer,
        default_request_id_header: HeaderName::from_static("x-request-id"),
        default_headers: HeaderMap::new(),
        capabilities: ProviderCapabilities {
            supports_streaming: true,
            supports_family_native_options: true,
            supports_provider_native_options: true,
        },
    }
}

pub(crate) fn openrouter_plan(
    execution: &ExecutionPlan,
) -> Result<ProviderRequestPlan, AdapterError> {
    let codec = codec_for(ProviderFamilyId::OpenAiCompatible);
    let refinement = refinement_for(ProviderKind::OpenRouter);
    let native_options = execution.provider_attempt.native_options.as_ref();
    let mut encoded = codec
        .encode_task(
            &execution.task,
            &execution.provider_attempt.model,
            execution.response_mode,
            native_options.and_then(|native| native.family.as_ref()),
        )
        .map_err(|error| rebind_adapter_error_provider(error, ProviderKind::OpenRouter))?;
    refinement.refine_request(
        &execution.task,
        &execution.provider_attempt.model,
        &mut encoded,
        native_options.and_then(|native| native.provider.as_ref()),
    )?;

    Ok(encoded.into())
}

impl ProviderAdapter for OpenRouterAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenRouter
    }
    fn descriptor(&self) -> &ProviderDescriptor {
        static DESCRIPTOR: std::sync::LazyLock<ProviderDescriptor> =
            std::sync::LazyLock::new(openrouter_descriptor);
        &DESCRIPTOR
    }
    fn plan_request(&self, execution: &ExecutionPlan) -> Result<ProviderRequestPlan, AdapterError> {
        plan_request_with_layering(self.kind(), self.descriptor().family, execution)
    }
    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        decode_response_with_layering(
            self.kind(),
            self.descriptor().family,
            body,
            requested_format,
        )
    }
    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        decode_error_with_layering(self.kind(), self.descriptor().family, body)
    }
    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        create_stream_projector_with_layering(self.kind(), self.descriptor().family)
    }
}
