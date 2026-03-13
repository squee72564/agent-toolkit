//! Built-in provider adapter implementations and adapter selection.

use reqwest::Url;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use agent_core::{
    AuthStyle, ProviderDescriptor, ProviderFamilyId, ProviderKind, Request, Response,
    ResponseFormat,
};

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
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

/// Provider-specific translation contract used by the runtime layer.
///
/// A `ProviderAdapter` is responsible for:
///
/// - exposing adapter-owned static provider metadata,
/// - translating provider-agnostic [`Request`] values into a
///   [`ProviderRequestPlan`],
/// - decoding provider JSON responses back into [`Response`], and
/// - projecting raw streaming events into canonical stream events.
pub trait ProviderAdapter: Sync + std::fmt::Debug {
    /// Returns the concrete provider kind implemented by this adapter.
    fn kind(&self) -> ProviderKind;
    /// Returns static metadata for this provider kind.
    fn descriptor(&self) -> &ProviderDescriptor;
    /// REFACTOR-SHIM: legacy accessor preserved while runtime callers migrate.
    fn default_base_url(&self) -> &'static str {
        self.descriptor().default_base_url
    }
    /// REFACTOR-SHIM: legacy accessor preserved while runtime callers migrate.
    fn endpoint_path(&self) -> &'static str {
        self.descriptor().endpoint_path
    }
    /// REFACTOR-SHIM: legacy helper preserved for tests and migration paths.
    fn platform_config(
        &self,
        base_url: String,
    ) -> Result<agent_core::PlatformConfig, AdapterError> {
        build_platform_config(self.kind(), self.descriptor(), base_url)
    }
    /// Translates a provider-agnostic request into a provider-specific request
    /// plan for the transport layer.
    fn plan_request(&self, req: Request) -> Result<ProviderRequestPlan, AdapterError>;
    /// Decodes a provider JSON response into the canonical response type.
    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError>;
    /// Creates a streaming projector for this provider.
    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector>;
}

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
    }
}

/// Built-in adapter for OpenAI-compatible response endpoints.
#[derive(Debug, Clone, Copy)]
pub struct OpenAiAdapter;

/// Built-in adapter for Anthropic message endpoints.
#[derive(Debug, Clone, Copy)]
pub struct AnthropicAdapter;

/// Built-in adapter for OpenRouter's OpenAI-compatible response endpoints.
#[derive(Debug, Clone, Copy)]
pub struct OpenRouterAdapter;

/// Built-in adapter for self-hosted OpenAI-compatible response endpoints.
#[derive(Debug, Clone, Copy)]
pub struct GenericOpenAiCompatibleAdapter;

static OPENAI_ADAPTER: OpenAiAdapter = OpenAiAdapter;
static ANTHROPIC_ADAPTER: AnthropicAdapter = AnthropicAdapter;
static OPENROUTER_ADAPTER: OpenRouterAdapter = OpenRouterAdapter;
static GENERIC_OPENAI_COMPATIBLE_ADAPTER: GenericOpenAiCompatibleAdapter =
    GenericOpenAiCompatibleAdapter;

/// Returns the built-in adapter for a provider kind.
///
/// The returned adapter is a shared `'static` singleton.
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

impl ProviderAdapter for OpenAiAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAi
    }

    fn descriptor(&self) -> &ProviderDescriptor {
        static DESCRIPTOR: std::sync::LazyLock<ProviderDescriptor> =
            std::sync::LazyLock::new(openai_descriptor);
        &DESCRIPTOR
    }

    fn plan_request(&self, req: Request) -> Result<ProviderRequestPlan, AdapterError> {
        openai_request::plan_request(req)
    }

    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        openai_response::decode_response_json(body, requested_format)
    }

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        Box::<openai_stream::OpenAiStreamProjector>::default()
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

    fn plan_request(&self, req: Request) -> Result<ProviderRequestPlan, AdapterError> {
        anthropic_request::plan_request(req)
    }

    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        anthropic_response::decode_response_json(body, requested_format)
    }

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        Box::<anthropic_stream::AnthropicStreamProjector>::default()
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

    fn plan_request(&self, req: Request) -> Result<ProviderRequestPlan, AdapterError> {
        openrouter_request::plan_request(req, &openrouter_request::OpenRouterOverrides::default())
    }

    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        openrouter_response::decode_response_json(body, requested_format)
    }

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        Box::<openrouter_stream::OpenRouterStreamProjector>::default()
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

    fn plan_request(&self, req: Request) -> Result<ProviderRequestPlan, AdapterError> {
        openai_request::plan_request(req)
    }

    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        openai_response::decode_response_json(body, requested_format)
    }

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        Box::<openai_stream::OpenAiStreamProjector>::default()
    }
}

fn build_platform_config(
    provider: ProviderKind,
    descriptor: &ProviderDescriptor,
    base_url: String,
) -> Result<agent_core::PlatformConfig, AdapterError> {
    let trimmed_base_url = base_url.trim().to_string();
    if trimmed_base_url.is_empty() {
        return Err(AdapterError::new(
            AdapterErrorKind::Validation,
            provider,
            AdapterOperation::BuildHttpRequest,
            format!("base_url is empty for provider {provider:?}"),
        ));
    }
    let parsed_base_url = Url::parse(trimmed_base_url.as_str()).map_err(|error| {
        AdapterError::new(
            AdapterErrorKind::Validation,
            provider,
            AdapterOperation::BuildHttpRequest,
            format!("base_url is not a valid URL for provider {provider:?}: {error}"),
        )
    })?;
    if !matches!(parsed_base_url.scheme(), "http" | "https") {
        return Err(AdapterError::new(
            AdapterErrorKind::Validation,
            provider,
            AdapterOperation::BuildHttpRequest,
            format!(
                "base_url must use http or https for provider {provider:?}, got scheme {}",
                parsed_base_url.scheme()
            ),
        ));
    }

    Ok(agent_core::PlatformConfig {
        protocol: descriptor.protocol.clone(),
        base_url: parsed_base_url
            .to_string()
            .trim_end_matches('/')
            .to_string(),
        auth_style: descriptor.default_auth_style.clone(),
        request_id_header: descriptor.default_request_id_header.clone(),
        default_headers: descriptor.default_headers.clone(),
    })
}
