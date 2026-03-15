//! Built-in provider adapter implementations and adapter selection.
use serde_json::Value;

use agent_core::{
    ExecutionPlan, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId, ProviderKind,
    Response, ResponseFormat,
};

use crate::adapter::{anthropic_plan, openrouter_plan};
use crate::error::{AdapterError, ProviderErrorInfo};
use crate::family_codec::codec_for;
use crate::overlay::overlay_for;
use crate::request_plan::ProviderRequestPlan;
use crate::stream_projector::ProviderStreamProjector;

pub trait ProviderAdapter: Sync + std::fmt::Debug {
    fn kind(&self) -> ProviderKind;
    fn descriptor(&self) -> &ProviderDescriptor;
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.descriptor().capabilities
    }
    fn plan_request(&self, execution: &ExecutionPlan) -> Result<ProviderRequestPlan, AdapterError>;
    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError>;
    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo>;
    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector>;
}

static OPENAI_ADAPTER: super::OpenAiAdapter = super::OpenAiAdapter;
static ANTHROPIC_ADAPTER: super::AnthropicAdapter = super::AnthropicAdapter;
static OPENROUTER_ADAPTER: super::OpenRouterAdapter = super::OpenRouterAdapter;
static GENERIC_OPENAI_COMPATIBLE_ADAPTER: super::GenericOpenAiCompatibleAdapter =
    super::GenericOpenAiCompatibleAdapter;

pub fn adapter_for(kind: ProviderKind) -> &'static dyn ProviderAdapter {
    match kind {
        ProviderKind::OpenAi => &OPENAI_ADAPTER,
        ProviderKind::Anthropic => &ANTHROPIC_ADAPTER,
        ProviderKind::OpenRouter => &OPENROUTER_ADAPTER,
        ProviderKind::GenericOpenAiCompatible => &GENERIC_OPENAI_COMPATIBLE_ADAPTER,
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn all_builtin_adapters() -> &'static [&'static dyn ProviderAdapter] {
    static ADAPTERS: [&'static dyn ProviderAdapter; 4] = [
        &OPENAI_ADAPTER,
        &ANTHROPIC_ADAPTER,
        &OPENROUTER_ADAPTER,
        &GENERIC_OPENAI_COMPATIBLE_ADAPTER,
    ];
    &ADAPTERS
}

pub(crate) fn plan_request_with_layering(
    provider: ProviderKind,
    family: ProviderFamilyId,
    execution: &ExecutionPlan,
) -> Result<ProviderRequestPlan, AdapterError> {
    match (family, provider) {
        (ProviderFamilyId::OpenAiCompatible, ProviderKind::OpenAi)
        | (ProviderFamilyId::OpenAiCompatible, ProviderKind::GenericOpenAiCompatible) => {
            super::openai_compatible_plan(execution, provider)
        }
        (ProviderFamilyId::OpenAiCompatible, ProviderKind::OpenRouter) => {
            openrouter_plan(execution)
        }
        (ProviderFamilyId::Anthropic, ProviderKind::Anthropic) => anthropic_plan(execution),
        (family, provider) => Err(AdapterError::new(
            crate::error::AdapterErrorKind::ProtocolViolation,
            provider,
            crate::error::AdapterOperation::PlanRequest,
            format!(
                "adapter {:?} does not support planning with provider family {:?}",
                provider, family
            ),
        )),
    }
}

pub(crate) fn decode_response_with_layering(
    provider: ProviderKind,
    family: ProviderFamilyId,
    body: Value,
    requested_format: &ResponseFormat,
) -> Result<Response, AdapterError> {
    let overlay = overlay_for(provider);
    let codec = codec_for(family);

    if let Some(result) = overlay.decode_response_override(body.clone(), requested_format) {
        return result;
    }

    codec
        .decode_response(body.clone(), requested_format)
        .map_err(|error| {
            let family_info = codec.decode_error(&body);
            let overlay_info = overlay.decode_provider_error(&body);
            apply_layered_error_info(
                rebind_adapter_error_provider(error, provider),
                family_info,
                overlay_info,
            )
        })
}

pub(crate) fn decode_error_with_layering(
    provider: ProviderKind,
    family: ProviderFamilyId,
    body: &Value,
) -> Option<ProviderErrorInfo> {
    let family_info = codec_for(family).decode_error(body);
    let overlay_info = overlay_for(provider).decode_provider_error(body);

    match (family_info, overlay_info) {
        (Some(family_info), Some(overlay_info)) => Some(family_info.refined_with(overlay_info)),
        (Some(family_info), None) => Some(family_info),
        (None, Some(overlay_info)) => Some(overlay_info),
        (None, None) => None,
    }
}

fn apply_layered_error_info(
    mut error: AdapterError,
    family_info: Option<ProviderErrorInfo>,
    overlay_info: Option<ProviderErrorInfo>,
) -> AdapterError {
    let info = match (family_info, overlay_info) {
        (Some(family_info), Some(overlay_info)) => Some(family_info.refined_with(overlay_info)),
        (Some(family_info), None) => Some(family_info),
        (None, Some(overlay_info)) => Some(overlay_info),
        (None, None) => None,
    };

    if let Some(info) = info {
        if let Some(kind) = info.kind {
            error.kind = kind;
        }
        if let Some(message) = info.message {
            error.message = message;
        }
        if let Some(provider_code) = info.provider_code {
            error.provider_code = Some(provider_code);
        }
    }

    error
}

pub(crate) fn rebind_adapter_error_provider(
    mut error: AdapterError,
    provider: ProviderKind,
) -> AdapterError {
    error.provider = provider;
    error
}

pub(crate) fn create_stream_projector_with_layering(
    provider: ProviderKind,
    family: ProviderFamilyId,
) -> Box<dyn ProviderStreamProjector> {
    let overlay = overlay_for(provider);
    let codec = codec_for(family);

    if let Some(projector) = overlay.create_stream_projector_override() {
        return projector;
    }

    codec.create_stream_projector()
}
