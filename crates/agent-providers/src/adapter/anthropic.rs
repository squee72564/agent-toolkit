use agent_core::{
    AuthStyle, ExecutionPlan, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId,
    ProviderKind, Response, ResponseFormat,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use crate::{
    adapter::{
        ProviderAdapter, create_stream_projector_with_layering, decode_error_with_layering,
        decode_response_with_layering, plan_request_with_layering, rebind_adapter_error_provider,
    },
    error::{AdapterError, ProviderErrorInfo},
    family_codec::codec_for,
    refinement::refinement_for,
    request_plan::ProviderRequestPlan,
    stream_projector::ProviderStreamProjector,
};

pub(crate) const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
pub(crate) const ANTHROPIC_ENDPOINT_PATH: &str = "/v1/messages";

#[derive(Debug, Clone, Copy)]
/// Built-in adapter for Anthropic's Messages API.
pub struct AnthropicAdapter;

pub(crate) fn anthropic_descriptor() -> ProviderDescriptor {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static("2023-06-01"),
    );

    ProviderDescriptor {
        kind: ProviderKind::Anthropic,
        family: ProviderFamilyId::Anthropic,
        protocol: agent_core::ProtocolKind::Anthropic,
        default_base_url: ANTHROPIC_BASE_URL,
        endpoint_path: ANTHROPIC_ENDPOINT_PATH,
        default_auth_style: AuthStyle::ApiKeyHeader(HeaderName::from_static("x-api-key")),
        default_request_id_header: HeaderName::from_static("request-id"),
        default_headers,
        capabilities: ProviderCapabilities {
            supports_streaming: true,
            supports_family_native_options: true,
            supports_provider_native_options: true,
        },
    }
}

pub(crate) fn anthropic_plan(
    execution: &ExecutionPlan,
) -> Result<ProviderRequestPlan, AdapterError> {
    let codec = codec_for(ProviderFamilyId::Anthropic);
    let refinement = refinement_for(ProviderKind::Anthropic);
    let native_options = execution.provider_attempt.native_options.as_ref();
    let mut encoded = codec
        .encode_task(
            &execution.task,
            &execution.provider_attempt.model,
            execution.response_mode,
            native_options.and_then(|native| native.family.as_ref()),
        )
        .map_err(|error| rebind_adapter_error_provider(error, ProviderKind::Anthropic))?;
    refinement.refine_request(
        &execution.task,
        &execution.provider_attempt.model,
        &mut encoded,
        native_options.and_then(|native| native.provider.as_ref()),
    )?;

    Ok(encoded.into())
}

impl ProviderAdapter for AnthropicAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }
    fn descriptor(&self) -> &ProviderDescriptor {
        static DESCRIPTOR: std::sync::LazyLock<ProviderDescriptor> =
            std::sync::LazyLock::new(anthropic_descriptor);
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
