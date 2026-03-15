use agent_core::{
    AuthStyle, ExecutionPlan, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId,
    ProviderKind, Response, ResponseFormat,
};
use reqwest::header::{HeaderMap, HeaderName};
use serde_json::Value;

use crate::{
    adapter::{
        ProviderAdapter, create_stream_projector_with_layering, decode_error_with_layering,
        decode_response_with_layering, plan_request_with_layering, rebind_adapter_error_provider,
    },
    error::{AdapterError, ProviderErrorInfo},
    family_codec::codec_for,
    overlay::overlay_for,
    request_plan::ProviderRequestPlan,
    stream_projector::ProviderStreamProjector,
};

pub(crate) const OPENAI_BASE_URL: &str = "https://api.openai.com";
pub(crate) const OPENAI_ENDPOINT_PATH: &str = "/v1/responses";

#[derive(Debug, Clone, Copy)]
pub struct OpenAiAdapter;

#[derive(Debug, Clone, Copy)]
pub struct GenericOpenAiCompatibleAdapter;

pub(crate) fn openai_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        kind: ProviderKind::OpenAi,
        family: ProviderFamilyId::OpenAiCompatible,
        protocol: agent_core::ProtocolKind::OpenAI,
        default_base_url: OPENAI_BASE_URL,
        endpoint_path: OPENAI_ENDPOINT_PATH,
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

pub(crate) fn generic_openai_compatible_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        kind: ProviderKind::GenericOpenAiCompatible,
        family: ProviderFamilyId::OpenAiCompatible,
        protocol: agent_core::ProtocolKind::OpenAI,
        default_base_url: OPENAI_BASE_URL,
        endpoint_path: OPENAI_ENDPOINT_PATH,
        default_auth_style: AuthStyle::Bearer,
        default_request_id_header: HeaderName::from_static("x-request-id"),
        default_headers: HeaderMap::new(),
        capabilities: ProviderCapabilities {
            supports_streaming: true,
            supports_family_native_options: true,
            supports_provider_native_options: false,
        },
    }
}

pub(crate) fn openai_compatible_plan(
    execution: &ExecutionPlan,
    provider: ProviderKind,
) -> Result<ProviderRequestPlan, AdapterError> {
    let codec = codec_for(ProviderFamilyId::OpenAiCompatible);
    let overlay = overlay_for(provider);
    let native_options = execution.provider_attempt.native_options.as_ref();
    let mut encoded = codec
        .encode_task(
            &execution.task,
            &execution.provider_attempt.model,
            execution.response_mode,
            native_options.and_then(|native| native.family.as_ref()),
        )
        .map_err(|error| rebind_adapter_error_provider(error, provider))?;
    overlay.apply_provider_overlay(
        &execution.task,
        &execution.provider_attempt.model,
        &mut encoded,
        native_options.and_then(|native| native.provider.as_ref()),
    )?;

    Ok(encoded.into())
}

impl ProviderAdapter for OpenAiAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAi
    }
    fn descriptor(&self) -> &ProviderDescriptor {
        static DESCRIPTOR: std::sync::LazyLock<ProviderDescriptor> =
            std::sync::LazyLock::new(openai_descriptor);
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

impl ProviderAdapter for GenericOpenAiCompatibleAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::GenericOpenAiCompatible
    }
    fn descriptor(&self) -> &ProviderDescriptor {
        static DESCRIPTOR: std::sync::LazyLock<ProviderDescriptor> =
            std::sync::LazyLock::new(generic_openai_compatible_descriptor);
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
