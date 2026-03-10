//! Built-in provider adapter implementations and adapter selection.

use reqwest::Url;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use agent_core::{
    AuthStyle, PlatformConfig, ProtocolKind, ProviderId, Request, Response, ResponseFormat,
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
/// - describing the provider's HTTP platform configuration,
/// - translating provider-agnostic [`Request`] values into a
///   [`ProviderRequestPlan`],
/// - decoding provider JSON responses back into [`Response`], and
/// - projecting raw streaming events into canonical stream events.
pub trait ProviderAdapter: Sync + std::fmt::Debug {
    /// Returns the provider identifier implemented by this adapter.
    fn id(&self) -> ProviderId;
    /// Returns the provider's default API base URL.
    fn default_base_url(&self) -> &'static str;
    /// Returns the default endpoint path used for requests.
    fn endpoint_path(&self) -> &'static str;
    /// Builds transport-facing platform configuration for the provider.
    fn platform_config(&self, base_url: String) -> Result<PlatformConfig, AdapterError>;
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

/// Built-in adapter for OpenAI-compatible response endpoints.
#[derive(Debug, Clone, Copy)]
pub struct OpenAiAdapter;

/// Built-in adapter for Anthropic message endpoints.
#[derive(Debug, Clone, Copy)]
pub struct AnthropicAdapter;

/// Built-in adapter for OpenRouter's OpenAI-compatible response endpoints.
#[derive(Debug, Clone, Copy)]
pub struct OpenRouterAdapter;

static OPENAI_ADAPTER: OpenAiAdapter = OpenAiAdapter;
static ANTHROPIC_ADAPTER: AnthropicAdapter = AnthropicAdapter;
static OPENROUTER_ADAPTER: OpenRouterAdapter = OpenRouterAdapter;

/// Returns the built-in adapter for a provider identifier.
///
/// The returned adapter is a shared `'static` singleton.
pub fn adapter_for(id: ProviderId) -> &'static dyn ProviderAdapter {
    match id {
        ProviderId::OpenAi => &OPENAI_ADAPTER,
        ProviderId::Anthropic => &ANTHROPIC_ADAPTER,
        ProviderId::OpenRouter => &OPENROUTER_ADAPTER,
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn all_builtin_adapters() -> &'static [&'static dyn ProviderAdapter] {
    static ADAPTERS: [&'static dyn ProviderAdapter; 3] =
        [&OPENAI_ADAPTER, &ANTHROPIC_ADAPTER, &OPENROUTER_ADAPTER];
    &ADAPTERS
}

impl ProviderAdapter for OpenAiAdapter {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAi
    }

    fn default_base_url(&self) -> &'static str {
        OPENAI_BASE_URL
    }

    fn endpoint_path(&self) -> &'static str {
        OPENAI_ENDPOINT_PATH
    }

    fn platform_config(&self, base_url: String) -> Result<PlatformConfig, AdapterError> {
        build_platform_config(
            self.id(),
            base_url,
            ProtocolKind::OpenAI,
            AuthStyle::Bearer,
            HeaderName::from_static("x-request-id"),
            HeaderMap::new(),
        )
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
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn default_base_url(&self) -> &'static str {
        ANTHROPIC_BASE_URL
    }

    fn endpoint_path(&self) -> &'static str {
        ANTHROPIC_ENDPOINT_PATH
    }

    fn platform_config(&self, base_url: String) -> Result<PlatformConfig, AdapterError> {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );

        build_platform_config(
            self.id(),
            base_url,
            ProtocolKind::Anthropic,
            AuthStyle::ApiKeyHeader(HeaderName::from_static("x-api-key")),
            HeaderName::from_static("request-id"),
            default_headers,
        )
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
    fn id(&self) -> ProviderId {
        ProviderId::OpenRouter
    }

    fn default_base_url(&self) -> &'static str {
        OPENROUTER_BASE_URL
    }

    fn endpoint_path(&self) -> &'static str {
        OPENROUTER_ENDPOINT_PATH
    }

    fn platform_config(&self, base_url: String) -> Result<PlatformConfig, AdapterError> {
        build_platform_config(
            self.id(),
            base_url,
            ProtocolKind::OpenAI,
            AuthStyle::Bearer,
            HeaderName::from_static("x-request-id"),
            HeaderMap::new(),
        )
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

fn build_platform_config(
    provider: ProviderId,
    base_url: String,
    protocol: ProtocolKind,
    auth_style: AuthStyle,
    request_id_header: HeaderName,
    default_headers: HeaderMap,
) -> Result<PlatformConfig, AdapterError> {
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

    Ok(PlatformConfig {
        protocol,
        base_url: parsed_base_url
            .to_string()
            .trim_end_matches('/')
            .to_string(),
        auth_style,
        request_id_header,
        default_headers,
    })
}
