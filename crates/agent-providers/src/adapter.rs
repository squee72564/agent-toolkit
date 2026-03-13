//! Built-in provider adapter implementations and adapter selection.

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use agent_core::{
    AuthStyle, ExecutionPlan, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId,
    ProviderKind, Response, ResponseFormat,
};

use crate::anthropic_family::{
    AnthropicDecodeEnvelope, AnthropicFamilyError, AnthropicFamilyErrorKind,
};
use crate::error::{AdapterError, ProviderErrorInfo};
use crate::openai_family::{OpenAiDecodeEnvelope, OpenAiFamilyError, OpenAiFamilyErrorKind};
use crate::platform::anthropic::{
    request as anthropic_request, response as anthropic_response, stream as anthropic_stream,
};
use crate::platform::openai::{
    request as openai_request, response as openai_response, stream as openai_stream,
};
use crate::platform::openrouter::{
    request as openrouter_request, response as openrouter_response, stream as openrouter_stream,
};
use crate::request_plan::ProviderRequestPlan;
use crate::streaming::ProviderStreamProjector;

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

#[cfg(test)]
mod test;

const OPENAI_BASE_URL: &str = "https://api.openai.com";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api";

const OPENAI_ENDPOINT_PATH: &str = "/v1/responses";
const ANTHROPIC_ENDPOINT_PATH: &str = "/v1/messages";
const OPENROUTER_ENDPOINT_PATH: &str = "/v1/responses";

fn openai_descriptor() -> ProviderDescriptor {
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

fn anthropic_descriptor() -> ProviderDescriptor {
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

fn openrouter_descriptor() -> ProviderDescriptor {
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

fn generic_openai_compatible_descriptor() -> ProviderDescriptor {
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

#[derive(Debug, Clone, Copy)]
pub struct OpenAiAdapter;
#[derive(Debug, Clone, Copy)]
pub struct AnthropicAdapter;
#[derive(Debug, Clone, Copy)]
pub struct OpenRouterAdapter;
#[derive(Debug, Clone, Copy)]
pub struct GenericOpenAiCompatibleAdapter;

static OPENAI_ADAPTER: OpenAiAdapter = OpenAiAdapter;
static ANTHROPIC_ADAPTER: AnthropicAdapter = AnthropicAdapter;
static OPENROUTER_ADAPTER: OpenRouterAdapter = OpenRouterAdapter;
static GENERIC_OPENAI_COMPATIBLE_ADAPTER: GenericOpenAiCompatibleAdapter =
    GenericOpenAiCompatibleAdapter;

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

fn openai_compatible_plan(
    execution: &ExecutionPlan,
    provider: ProviderKind,
) -> Result<ProviderRequestPlan, AdapterError> {
    let family_options = execution
        .provider_attempt
        .native_options
        .as_ref()
        .and_then(|native| native.family.as_ref())
        .and_then(|family| match family {
            agent_core::FamilyOptions::OpenAiCompatible(options) => Some(options),
            _ => None,
        });
    let provider_options = execution
        .provider_attempt
        .native_options
        .as_ref()
        .and_then(|native| native.provider.as_ref());
    let mut encoded = openai_request::encode_family_request(
        &execution.task,
        &execution.provider_attempt.model,
        execution.response_mode,
    )
    .map_err(|error| rebind_adapter_error_provider(error, provider))?;
    openai_request::apply_provider_overlay(
        provider,
        &mut encoded,
        family_options,
        provider_options,
    )?;

    Ok(encoded.into())
}

fn anthropic_plan(execution: &ExecutionPlan) -> Result<ProviderRequestPlan, AdapterError> {
    let family_options = execution
        .provider_attempt
        .native_options
        .as_ref()
        .and_then(|native| native.family.as_ref())
        .and_then(|family| match family {
            agent_core::FamilyOptions::Anthropic(options) => Some(options),
            _ => None,
        });
    let provider_options = execution
        .provider_attempt
        .native_options
        .as_ref()
        .and_then(|native| native.provider.as_ref());
    let mut encoded = anthropic_request::encode_family_request(
        &execution.task,
        &execution.provider_attempt.model,
        execution.response_mode,
    )?;
    anthropic_request::apply_provider_overlay(&mut encoded, family_options, provider_options)?;

    Ok(encoded.into())
}

fn openrouter_plan(execution: &ExecutionPlan) -> Result<ProviderRequestPlan, AdapterError> {
    let family_options = execution
        .provider_attempt
        .native_options
        .as_ref()
        .and_then(|native| native.family.as_ref())
        .and_then(|family| match family {
            agent_core::FamilyOptions::OpenAiCompatible(options) => Some(options),
            _ => None,
        });
    let provider_options = execution
        .provider_attempt
        .native_options
        .as_ref()
        .and_then(|native| native.provider.as_ref());
    let mut encoded = openai_request::encode_family_request(
        &execution.task,
        &execution.provider_attempt.model,
        execution.response_mode,
    )
    .map_err(|error| rebind_adapter_error_provider(error, ProviderKind::OpenRouter))?;
    openrouter_request::apply_provider_overlay(
        &mut encoded,
        &execution.provider_attempt.model,
        execution.task.top_p,
        &execution.task.stop,
        family_options,
        provider_options,
    )?;

    Ok(encoded.into())
}

fn plan_request_with_layering(
    provider: ProviderKind,
    family: ProviderFamilyId,
    execution: &ExecutionPlan,
) -> Result<ProviderRequestPlan, AdapterError> {
    match (family, provider) {
        (ProviderFamilyId::OpenAiCompatible, ProviderKind::OpenAi)
        | (ProviderFamilyId::OpenAiCompatible, ProviderKind::GenericOpenAiCompatible) => {
            openai_compatible_plan(execution, provider)
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

fn decode_response_with_layering(
    provider: ProviderKind,
    family: ProviderFamilyId,
    body: Value,
    requested_format: &ResponseFormat,
) -> Result<Response, AdapterError> {
    if let Some(result) = decode_response_override(provider, body.clone(), requested_format) {
        return result;
    }

    match family {
        ProviderFamilyId::OpenAiCompatible => {
            crate::openai_family::decode::decode_openai_response(&OpenAiDecodeEnvelope {
                body: body.clone(),
                requested_response_format: requested_format.clone(),
            })
            .map_err(|error| refine_openai_family_decode_error(provider, family, &body, error))
        }
        ProviderFamilyId::Anthropic => {
            crate::anthropic_family::decode::decode_anthropic_response(&AnthropicDecodeEnvelope {
                body: body.clone(),
                requested_response_format: requested_format.clone(),
            })
            .map_err(|error| refine_anthropic_family_decode_error(provider, family, &body, error))
        }
    }
}

fn decode_response_override(
    provider: ProviderKind,
    body: Value,
    requested_format: &ResponseFormat,
) -> Option<Result<Response, AdapterError>> {
    match provider {
        ProviderKind::OpenAi | ProviderKind::GenericOpenAiCompatible => {
            openai_response::decode_response_override(provider, body, requested_format)
        }
        ProviderKind::Anthropic => {
            anthropic_response::decode_response_override(body, requested_format)
        }
        ProviderKind::OpenRouter => {
            openrouter_response::decode_response_override(body, requested_format)
        }
    }
}

fn refine_openai_family_decode_error(
    provider: ProviderKind,
    family: ProviderFamilyId,
    body: &Value,
    error: OpenAiFamilyError,
) -> AdapterError {
    let message = error.message().to_string();
    apply_layered_error_info(
        provider,
        family,
        body,
        AdapterError::with_source(
            map_openai_family_error_kind(error.kind()),
            provider,
            crate::error::AdapterOperation::DecodeResponse,
            message,
            error,
        ),
    )
}

fn refine_anthropic_family_decode_error(
    provider: ProviderKind,
    family: ProviderFamilyId,
    body: &Value,
    error: AnthropicFamilyError,
) -> AdapterError {
    let message = error.message().to_string();
    apply_layered_error_info(
        provider,
        family,
        body,
        AdapterError::with_source(
            map_anthropic_family_error_kind(error.kind()),
            provider,
            crate::error::AdapterOperation::DecodeResponse,
            message,
            error,
        ),
    )
}

fn decode_error_with_layering(
    provider: ProviderKind,
    family: ProviderFamilyId,
    body: &Value,
) -> Option<ProviderErrorInfo> {
    let family_info = match family {
        ProviderFamilyId::OpenAiCompatible => {
            crate::openai_family::decode::decode_openai_error(body)
        }
        ProviderFamilyId::Anthropic => {
            crate::anthropic_family::decode::decode_anthropic_error(body)
        }
    };
    let overlay_info = decode_provider_error_override(provider, body);

    match (family_info, overlay_info) {
        (Some(family_info), Some(overlay_info)) => Some(family_info.refined_with(overlay_info)),
        (Some(family_info), None) => Some(family_info),
        (None, Some(overlay_info)) => Some(overlay_info),
        (None, None) => None,
    }
}

fn decode_provider_error_override(
    provider: ProviderKind,
    body: &Value,
) -> Option<ProviderErrorInfo> {
    match provider {
        ProviderKind::OpenAi | ProviderKind::GenericOpenAiCompatible => {
            openai_response::decode_provider_error(body)
        }
        ProviderKind::Anthropic => anthropic_response::decode_provider_error(body),
        ProviderKind::OpenRouter => openrouter_response::decode_provider_error(body),
    }
}

fn apply_layered_error_info(
    provider: ProviderKind,
    family: ProviderFamilyId,
    body: &Value,
    mut error: AdapterError,
) -> AdapterError {
    if let Some(info) = decode_error_with_layering(provider, family, body) {
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

    match provider {
        ProviderKind::OpenAi | ProviderKind::GenericOpenAiCompatible => {
            openai_response::refine_family_decode_error(body, error)
        }
        ProviderKind::Anthropic => anthropic_response::refine_family_decode_error(body, error),
        ProviderKind::OpenRouter => openrouter_response::refine_family_decode_error(body, error),
    }
}

fn map_openai_family_error_kind(kind: OpenAiFamilyErrorKind) -> crate::error::AdapterErrorKind {
    match kind {
        OpenAiFamilyErrorKind::Validation => crate::error::AdapterErrorKind::Validation,
        OpenAiFamilyErrorKind::Encode => crate::error::AdapterErrorKind::Encode,
        OpenAiFamilyErrorKind::Decode => crate::error::AdapterErrorKind::Decode,
        OpenAiFamilyErrorKind::Upstream => crate::error::AdapterErrorKind::Upstream,
        OpenAiFamilyErrorKind::ProtocolViolation => {
            crate::error::AdapterErrorKind::ProtocolViolation
        }
        OpenAiFamilyErrorKind::UnsupportedFeature => {
            crate::error::AdapterErrorKind::UnsupportedFeature
        }
    }
}

fn map_anthropic_family_error_kind(
    kind: AnthropicFamilyErrorKind,
) -> crate::error::AdapterErrorKind {
    match kind {
        AnthropicFamilyErrorKind::Validation => crate::error::AdapterErrorKind::Validation,
        AnthropicFamilyErrorKind::Encode => crate::error::AdapterErrorKind::Encode,
        AnthropicFamilyErrorKind::Decode => crate::error::AdapterErrorKind::Decode,
        AnthropicFamilyErrorKind::Upstream => crate::error::AdapterErrorKind::Upstream,
        AnthropicFamilyErrorKind::ProtocolViolation => {
            crate::error::AdapterErrorKind::ProtocolViolation
        }
        AnthropicFamilyErrorKind::UnsupportedFeature => {
            crate::error::AdapterErrorKind::UnsupportedFeature
        }
    }
}

fn rebind_adapter_error_provider(mut error: AdapterError, provider: ProviderKind) -> AdapterError {
    error.provider = provider;
    error
}

fn create_stream_projector_with_layering(
    provider: ProviderKind,
    family: ProviderFamilyId,
) -> Box<dyn ProviderStreamProjector> {
    if let Some(projector) = create_stream_projector_override(provider) {
        return projector;
    }

    match family {
        ProviderFamilyId::OpenAiCompatible => {
            Box::<openai_stream::OpenAiStreamProjector>::default()
        }
        ProviderFamilyId::Anthropic => Box::<anthropic_stream::AnthropicStreamProjector>::default(),
    }
}

fn create_stream_projector_override(
    provider: ProviderKind,
) -> Option<Box<dyn ProviderStreamProjector>> {
    match provider {
        ProviderKind::OpenRouter => {
            Some(Box::<openrouter_stream::OpenRouterStreamProjector>::default())
        }
        ProviderKind::OpenAi | ProviderKind::Anthropic | ProviderKind::GenericOpenAiCompatible => {
            None
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn decode_response_with_composition_test_hook<Overlay, Family, Refine>(
    body: Value,
    requested_format: &ResponseFormat,
    overlay_decode: Overlay,
    family_decode: Family,
    refine_family_error: Refine,
) -> Result<Response, AdapterError>
where
    Overlay: FnOnce(Value, &ResponseFormat) -> Option<Result<Response, AdapterError>>,
    Family: FnOnce(Value, &ResponseFormat) -> Result<Response, AdapterError>,
    Refine: FnOnce(AdapterError) -> AdapterError,
{
    if let Some(result) = overlay_decode(body.clone(), requested_format) {
        return result;
    }

    family_decode(body, requested_format).map_err(refine_family_error)
}

#[cfg(test)]
#[allow(dead_code)]
fn decode_error_with_composition_test_hook<Family, Overlay>(
    family_decode: Family,
    overlay_decode: Overlay,
) -> Option<ProviderErrorInfo>
where
    Family: FnOnce() -> Option<ProviderErrorInfo>,
    Overlay: FnOnce() -> Option<ProviderErrorInfo>,
{
    let family_info = family_decode();
    let overlay_info = overlay_decode();

    match (family_info, overlay_info) {
        (Some(family_info), Some(overlay_info)) => Some(family_info.refined_with(overlay_info)),
        (Some(family_info), None) => Some(family_info),
        (None, Some(overlay_info)) => Some(overlay_info),
        (None, None) => None,
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn create_stream_projector_with_composition_test_hook<Overlay, Family>(
    overlay_projector: Overlay,
    family_projector: Family,
) -> Box<dyn ProviderStreamProjector>
where
    Overlay: FnOnce() -> Option<Box<dyn ProviderStreamProjector>>,
    Family: FnOnce() -> Box<dyn ProviderStreamProjector>,
{
    overlay_projector().unwrap_or_else(family_projector)
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
