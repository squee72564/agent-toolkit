use std::collections::{BTreeMap, HashMap};
use std::error::Error as StdError;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use thiserror::Error;

use agent_core::types::{
    AdapterContext, AuthCredentials, AuthStyle, Message, MessageRole, PlatformConfig, ProtocolKind,
    Request, Response, ResponseFormat, ToolChoice, ToolDefinition,
};
use agent_providers::anthropic_spec::AnthropicDecodeEnvelope;
use agent_providers::error::{AdapterError, AdapterErrorKind, AdapterProtocol};
use agent_providers::openai_spec::OpenAiDecodeEnvelope;
use agent_providers::platform::anthropic::translator::AnthropicTranslator;
use agent_providers::platform::openai::translator::OpenAiTranslator;
use agent_providers::platform::openrouter::translator::OpenRouterTranslator;
use agent_providers::translator_contract::ProtocolTranslator;
use agent_transport::{HttpJsonResponse, HttpTransport, RetryPolicy, TransportError};

const OPENAI_BASE_URL: &str = "https://api.openai.com";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api";

const OPENAI_ENDPOINT_PATH: &str = "/v1/responses";
const ANTHROPIC_ENDPOINT_PATH: &str = "/v1/messages";
const OPENROUTER_ENDPOINT_PATH: &str = "/v1/chat/completions";

const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const OPENAI_BASE_URL_ENV: &str = "OPENAI_BASE_URL";
const OPENAI_MODEL_ENV: &str = "OPENAI_MODEL";
const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const ANTHROPIC_BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
const ANTHROPIC_MODEL_ENV: &str = "ANTHROPIC_MODEL";
const OPENROUTER_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
const OPENROUTER_BASE_URL_ENV: &str = "OPENROUTER_BASE_URL";
const OPENROUTER_MODEL_ENV: &str = "OPENROUTER_MODEL";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderId {
    OpenAi,
    Anthropic,
    OpenRouter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub provider: ProviderId,
    pub model: Option<String>,
}

impl Target {
    pub fn new(provider: ProviderId) -> Self {
        Self {
            provider,
            model: None,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackPolicy {
    pub targets: Vec<Target>,
    pub retry_on_status_codes: Vec<u16>,
    pub retry_on_transport_error: bool,
}

impl FallbackPolicy {
    pub fn new(targets: Vec<Target>) -> Self {
        Self {
            targets,
            ..Self::default()
        }
    }

    fn should_fallback(&self, error: &RuntimeError) -> bool {
        if self.retry_on_transport_error && error.kind == RuntimeErrorKind::Transport {
            return true;
        }

        if let Some(status_code) = error.status_code {
            return self.retry_on_status_codes.contains(&status_code);
        }

        false
    }
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            retry_on_status_codes: vec![429, 500, 502, 503, 504],
            retry_on_transport_error: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendOptions {
    pub target: Option<Target>,
    pub fallback_policy: Option<FallbackPolicy>,
    pub metadata: BTreeMap<String, String>,
}

impl SendOptions {
    pub fn for_target(target: Target) -> Self {
        Self {
            target: Some(target),
            ..Self::default()
        }
    }

    pub fn with_fallback_policy(mut self, fallback_policy: FallbackPolicy) -> Self {
        self.fallback_policy = Some(fallback_policy);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    Configuration,
    TargetResolution,
    FallbackExhausted,
    Validation,
    Encode,
    Decode,
    ProtocolViolation,
    UnsupportedFeature,
    Upstream,
    Transport,
}

#[derive(Debug, Error)]
#[error("{kind:?}: {message}")]
pub struct RuntimeError {
    pub kind: RuntimeErrorKind,
    pub message: String,
    pub provider: Option<ProviderId>,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub provider_code: Option<String>,
    #[source]
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl RuntimeError {
    fn configuration(message: impl Into<String>) -> Self {
        Self {
            kind: RuntimeErrorKind::Configuration,
            message: message.into(),
            provider: None,
            status_code: None,
            request_id: None,
            provider_code: None,
            source: None,
        }
    }

    fn target_resolution(message: impl Into<String>) -> Self {
        Self {
            kind: RuntimeErrorKind::TargetResolution,
            message: message.into(),
            provider: None,
            status_code: None,
            request_id: None,
            provider_code: None,
            source: None,
        }
    }

    fn fallback_exhausted(last_error: RuntimeError) -> Self {
        Self {
            kind: RuntimeErrorKind::FallbackExhausted,
            message: format!("fallback attempts exhausted: {}", last_error.message),
            provider: last_error.provider,
            status_code: last_error.status_code,
            request_id: last_error.request_id.clone(),
            provider_code: last_error.provider_code.clone(),
            source: Some(Box::new(last_error)),
        }
    }

    fn from_adapter(error: AdapterError) -> Self {
        let status_code = error.status_code;
        let request_id = error.request_id.clone();
        let provider_code = error.provider_code.clone();

        Self {
            kind: map_adapter_error_kind(error.kind),
            message: error.message.clone(),
            provider: Some(map_adapter_protocol(error.protocol)),
            status_code,
            request_id,
            provider_code,
            source: Some(Box::new(error)),
        }
    }

    fn from_transport(provider: ProviderId, error: TransportError) -> Self {
        let (message, status_code) = match &error {
            TransportError::InvalidHeaderName => ("invalid header name".to_string(), None),
            TransportError::InvalidHeaderValue => ("invalid header value".to_string(), None),
            TransportError::Serialization => ("request serialization failed".to_string(), None),
            TransportError::Request(reqwest_error) => (
                reqwest_error.to_string(),
                reqwest_error.status().map(|status| status.as_u16()),
            ),
        };

        Self {
            kind: RuntimeErrorKind::Transport,
            message,
            provider: Some(provider),
            status_code,
            request_id: None,
            provider_code: None,
            source: Some(Box::new(error)),
        }
    }

    pub fn source_ref(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptMeta {
    pub provider: ProviderId,
    pub model: String,
    pub success: bool,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub error_kind: Option<RuntimeErrorKind>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseMeta {
    pub selected_provider: ProviderId,
    pub selected_model: String,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub attempts: Vec<AttemptMeta>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageCreateInput {
    pub model: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
    pub response_format: ResponseFormat,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub stop: Vec<String>,
    pub metadata: BTreeMap<String, String>,
}

impl MessageCreateInput {
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            model: None,
            messages,
            tools: Vec::new(),
            tool_choice: ToolChoice::default(),
            response_format: ResponseFormat::default(),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            stop: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::from(text.into())
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    fn into_request_with_options(
        self,
        default_model: Option<&str>,
        allow_empty_model: bool,
    ) -> Result<Request, RuntimeError> {
        if self.messages.is_empty() {
            return Err(RuntimeError::configuration(
                "messages().create(...) requires at least one message",
            ));
        }

        let model_id = match (self.model, default_model) {
            (Some(model_id), _) if !model_id.trim().is_empty() => model_id,
            (_, Some(default_model)) if !default_model.trim().is_empty() => {
                default_model.to_string()
            }
            _ if allow_empty_model => String::new(),
            _ => {
                return Err(RuntimeError::configuration(
                    "no model was provided and no default model is configured",
                ));
            }
        };

        Ok(Request {
            model_id,
            messages: self.messages,
            tools: self.tools,
            tool_choice: self.tool_choice,
            response_format: self.response_format,
            temperature: self.temperature,
            top_p: self.top_p,
            max_output_tokens: self.max_output_tokens,
            stop: self.stop,
            metadata: self.metadata,
        })
    }
}

impl From<String> for MessageCreateInput {
    fn from(text: String) -> Self {
        Self::new(vec![Message {
            role: MessageRole::User,
            content: vec![agent_core::types::ContentPart::Text { text }],
        }])
    }
}

impl From<&str> for MessageCreateInput {
    fn from(text: &str) -> Self {
        Self::from(text.to_string())
    }
}

impl From<Vec<Message>> for MessageCreateInput {
    fn from(messages: Vec<Message>) -> Self {
        Self::new(messages)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub api_key: String,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub retry_policy: Option<RetryPolicy>,
    pub timeout: Option<Duration>,
}

impl ProviderConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            ..Self::default()
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn with_default_model(mut self, default_model: impl Into<String>) -> Self {
        self.default_model = Some(default_model.into());
        self
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = Some(retry_policy);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiClient {
    inner: ProviderClient,
}

impl OpenAiClient {
    pub fn builder() -> OpenAiClientBuilder {
        OpenAiClientBuilder::default()
    }

    pub fn from_env() -> Result<Self, RuntimeError> {
        let _ = dotenvy::dotenv();

        let mut builder = Self::builder().api_key(require_env(OPENAI_API_KEY_ENV)?);
        if let Some(base_url) = read_env(OPENAI_BASE_URL_ENV) {
            builder = builder.base_url(base_url);
        }
        if let Some(default_model) = read_env(OPENAI_MODEL_ENV) {
            builder = builder.default_model(default_model);
        }

        builder.build()
    }

    pub fn messages(&self) -> MessagesApi<'_> {
        self.inner.messages()
    }

    pub async fn send(&self, request: Request) -> Result<Response, RuntimeError> {
        self.inner.send(request).await
    }

    pub async fn send_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.inner.send_with_meta(request).await
    }
}

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    inner: ProviderClient,
}

impl AnthropicClient {
    pub fn builder() -> AnthropicClientBuilder {
        AnthropicClientBuilder::default()
    }

    pub fn from_env() -> Result<Self, RuntimeError> {
        let _ = dotenvy::dotenv();

        let mut builder = Self::builder().api_key(require_env(ANTHROPIC_API_KEY_ENV)?);
        if let Some(base_url) = read_env(ANTHROPIC_BASE_URL_ENV) {
            builder = builder.base_url(base_url);
        }
        if let Some(default_model) = read_env(ANTHROPIC_MODEL_ENV) {
            builder = builder.default_model(default_model);
        }

        builder.build()
    }

    pub fn messages(&self) -> MessagesApi<'_> {
        self.inner.messages()
    }

    pub async fn send(&self, request: Request) -> Result<Response, RuntimeError> {
        self.inner.send(request).await
    }

    pub async fn send_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.inner.send_with_meta(request).await
    }
}

#[derive(Debug, Clone)]
pub struct OpenRouterClient {
    inner: ProviderClient,
}

impl OpenRouterClient {
    pub fn builder() -> OpenRouterClientBuilder {
        OpenRouterClientBuilder::default()
    }

    pub fn from_env() -> Result<Self, RuntimeError> {
        let _ = dotenvy::dotenv();

        let mut builder = Self::builder().api_key(require_env(OPENROUTER_API_KEY_ENV)?);
        if let Some(base_url) = read_env(OPENROUTER_BASE_URL_ENV) {
            builder = builder.base_url(base_url);
        }
        if let Some(default_model) = read_env(OPENROUTER_MODEL_ENV) {
            builder = builder.default_model(default_model);
        }

        builder.build()
    }

    pub fn messages(&self) -> MessagesApi<'_> {
        self.inner.messages()
    }

    pub async fn send(&self, request: Request) -> Result<Response, RuntimeError> {
        self.inner.send(request).await
    }

    pub async fn send_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.inner.send_with_meta(request).await
    }
}

pub fn openai() -> OpenAiClientBuilder {
    OpenAiClient::builder()
}

pub fn anthropic() -> AnthropicClientBuilder {
    AnthropicClient::builder()
}

pub fn openrouter() -> OpenRouterClientBuilder {
    OpenRouterClient::builder()
}

#[derive(Debug, Clone, Default)]
pub struct OpenAiClientBuilder {
    inner: BaseClientBuilder,
}

impl OpenAiClientBuilder {
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.inner.api_key = Some(api_key.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.inner.base_url = Some(base_url.into());
        self
    }

    pub fn default_model(mut self, default_model: impl Into<String>) -> Self {
        self.inner.default_model = Some(default_model.into());
        self
    }

    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.inner.retry_policy = Some(retry_policy);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner.timeout = Some(timeout);
        self
    }

    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.inner.client = Some(client);
        self
    }

    pub fn build(self) -> Result<OpenAiClient, RuntimeError> {
        let provider_runtime = self.inner.build_runtime(ProviderId::OpenAi)?;
        Ok(OpenAiClient {
            inner: ProviderClient::new(provider_runtime),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnthropicClientBuilder {
    inner: BaseClientBuilder,
}

impl AnthropicClientBuilder {
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.inner.api_key = Some(api_key.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.inner.base_url = Some(base_url.into());
        self
    }

    pub fn default_model(mut self, default_model: impl Into<String>) -> Self {
        self.inner.default_model = Some(default_model.into());
        self
    }

    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.inner.retry_policy = Some(retry_policy);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner.timeout = Some(timeout);
        self
    }

    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.inner.client = Some(client);
        self
    }

    pub fn build(self) -> Result<AnthropicClient, RuntimeError> {
        let provider_runtime = self.inner.build_runtime(ProviderId::Anthropic)?;
        Ok(AnthropicClient {
            inner: ProviderClient::new(provider_runtime),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct OpenRouterClientBuilder {
    inner: BaseClientBuilder,
}

impl OpenRouterClientBuilder {
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.inner.api_key = Some(api_key.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.inner.base_url = Some(base_url.into());
        self
    }

    pub fn default_model(mut self, default_model: impl Into<String>) -> Self {
        self.inner.default_model = Some(default_model.into());
        self
    }

    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.inner.retry_policy = Some(retry_policy);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner.timeout = Some(timeout);
        self
    }

    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.inner.client = Some(client);
        self
    }

    pub fn build(self) -> Result<OpenRouterClient, RuntimeError> {
        let provider_runtime = self.inner.build_runtime(ProviderId::OpenRouter)?;
        Ok(OpenRouterClient {
            inner: ProviderClient::new(provider_runtime),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MessagesApi<'a> {
    client: &'a ProviderClient,
}

impl MessagesApi<'_> {
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
    ) -> Result<Response, RuntimeError> {
        self.client.create(input.into()).await
    }

    pub async fn create_with_meta(
        &self,
        input: impl Into<MessageCreateInput>,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.client.create_with_meta(input.into()).await
    }

    pub async fn create_request(&self, request: Request) -> Result<Response, RuntimeError> {
        self.client.send(request).await
    }

    pub async fn create_request_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.client.send_with_meta(request).await
    }
}

#[derive(Debug, Clone)]
pub struct AgentToolkit {
    clients: HashMap<ProviderId, ProviderClient>,
}

impl AgentToolkit {
    pub fn builder() -> AgentToolkitBuilder {
        AgentToolkitBuilder::default()
    }

    pub fn messages(&self) -> RouterMessagesApi<'_> {
        RouterMessagesApi { toolkit: self }
    }

    pub async fn send(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<Response, RuntimeError> {
        self.send_with_meta(request, options)
            .await
            .map(|(response, _)| response)
    }

    pub async fn send_with_meta(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let targets = self.resolve_targets(&options)?;
        let fallback_policy = options.fallback_policy.clone();
        let mut attempts = Vec::new();
        let mut last_error: Option<RuntimeError> = None;

        for (index, target) in targets.iter().enumerate() {
            let Some(client) = self.clients.get(&target.provider) else {
                return Err(RuntimeError::target_resolution(format!(
                    "provider {:?} is not registered",
                    target.provider
                )));
            };

            let attempt = client
                .runtime
                .execute_attempt(
                    request.clone(),
                    target.model.as_deref(),
                    options.metadata.clone(),
                )
                .await;

            match attempt {
                ProviderAttemptOutcome::Success { response, meta } => {
                    attempts.push(meta.clone());
                    let response_meta = ResponseMeta {
                        selected_provider: meta.provider,
                        selected_model: meta.model,
                        status_code: meta.status_code,
                        request_id: meta.request_id.clone(),
                        attempts,
                    };
                    return Ok((response, response_meta));
                }
                ProviderAttemptOutcome::Failure { error, meta } => {
                    attempts.push(meta);
                    let should_continue = index + 1 < targets.len()
                        && fallback_policy
                            .as_ref()
                            .is_some_and(|policy| policy.should_fallback(&error));
                    last_error = Some(error);
                    if !should_continue {
                        break;
                    }
                }
            }
        }

        match last_error {
            Some(error) if attempts.len() > 1 => Err(RuntimeError::fallback_exhausted(error)),
            Some(error) => Err(error),
            None => Err(RuntimeError::target_resolution(
                "no target providers were resolved for this request",
            )),
        }
    }

    fn resolve_targets(&self, options: &SendOptions) -> Result<Vec<Target>, RuntimeError> {
        let mut targets = Vec::new();

        if let Some(primary_target) = &options.target {
            targets.push(primary_target.clone());
            if let Some(fallback_policy) = &options.fallback_policy {
                for target in &fallback_policy.targets {
                    if *target != *primary_target {
                        targets.push(target.clone());
                    }
                }
            }
        } else if let Some(fallback_policy) = &options.fallback_policy {
            if fallback_policy.targets.is_empty() {
                return Err(RuntimeError::target_resolution(
                    "fallback policy requires at least one target",
                ));
            }
            targets.extend(fallback_policy.targets.clone());
        } else {
            return Err(RuntimeError::target_resolution(
                "explicit target is required unless a fallback policy is provided",
            ));
        }

        for target in &targets {
            if !self.clients.contains_key(&target.provider) {
                return Err(RuntimeError::target_resolution(format!(
                    "provider {:?} is not registered",
                    target.provider
                )));
            }
        }

        Ok(targets)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AgentToolkitBuilder {
    openai: Option<ProviderConfig>,
    anthropic: Option<ProviderConfig>,
    openrouter: Option<ProviderConfig>,
}

impl AgentToolkitBuilder {
    pub fn with_openai(mut self, config: ProviderConfig) -> Self {
        self.openai = Some(config);
        self
    }

    pub fn with_anthropic(mut self, config: ProviderConfig) -> Self {
        self.anthropic = Some(config);
        self
    }

    pub fn with_openrouter(mut self, config: ProviderConfig) -> Self {
        self.openrouter = Some(config);
        self
    }

    pub fn build(self) -> Result<AgentToolkit, RuntimeError> {
        let mut clients = HashMap::new();

        if let Some(config) = self.openai {
            let runtime = BaseClientBuilder::from_provider_config(config)
                .build_runtime(ProviderId::OpenAi)?;
            clients.insert(ProviderId::OpenAi, ProviderClient::new(runtime));
        }
        if let Some(config) = self.anthropic {
            let runtime = BaseClientBuilder::from_provider_config(config)
                .build_runtime(ProviderId::Anthropic)?;
            clients.insert(ProviderId::Anthropic, ProviderClient::new(runtime));
        }
        if let Some(config) = self.openrouter {
            let runtime = BaseClientBuilder::from_provider_config(config)
                .build_runtime(ProviderId::OpenRouter)?;
            clients.insert(ProviderId::OpenRouter, ProviderClient::new(runtime));
        }

        if clients.is_empty() {
            return Err(RuntimeError::configuration(
                "at least one provider must be configured",
            ));
        }

        Ok(AgentToolkit { clients })
    }
}

#[derive(Debug, Clone)]
pub struct RouterMessagesApi<'a> {
    toolkit: &'a AgentToolkit,
}

impl RouterMessagesApi<'_> {
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
        options: SendOptions,
    ) -> Result<Response, RuntimeError> {
        self.create_with_meta(input, options)
            .await
            .map(|(response, _)| response)
    }

    pub async fn create_with_meta(
        &self,
        input: impl Into<MessageCreateInput>,
        options: SendOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let request = input.into().into_request_with_options(None, true)?;
        self.toolkit.send_with_meta(request, options).await
    }

    pub async fn create_request(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<Response, RuntimeError> {
        self.toolkit.send(request, options).await
    }

    pub async fn create_request_with_meta(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.toolkit.send_with_meta(request, options).await
    }
}

#[derive(Debug, Clone, Default)]
struct BaseClientBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    default_model: Option<String>,
    retry_policy: Option<RetryPolicy>,
    timeout: Option<Duration>,
    client: Option<reqwest::Client>,
}

impl BaseClientBuilder {
    fn from_provider_config(config: ProviderConfig) -> Self {
        Self {
            api_key: Some(config.api_key),
            base_url: config.base_url,
            default_model: config.default_model,
            retry_policy: config.retry_policy,
            timeout: config.timeout,
            client: None,
        }
    }

    fn build_runtime(self, provider: ProviderId) -> Result<ProviderRuntime, RuntimeError> {
        let api_key = self.api_key.ok_or_else(|| {
            RuntimeError::configuration(format!("missing API key for provider {provider:?}"))
        })?;
        if api_key.trim().is_empty() {
            return Err(RuntimeError::configuration(format!(
                "API key is empty for provider {provider:?}"
            )));
        }

        let reqwest_client = if let Some(client) = self.client {
            client
        } else {
            reqwest::Client::builder()
                .build()
                .map_err(|error| RuntimeError::configuration(error.to_string()))?
        };

        let mut transport_builder = HttpTransport::builder(reqwest_client);
        if let Some(retry_policy) = self.retry_policy {
            transport_builder = transport_builder.retry_policy(retry_policy);
        }
        if let Some(timeout) = self.timeout {
            transport_builder = transport_builder.timeout(timeout);
        }

        let transport = transport_builder.build();
        let base_url = self
            .base_url
            .unwrap_or_else(|| default_base_url_for_provider(provider).to_string());
        let platform = platform_config_for_provider(provider, base_url)?;

        Ok(ProviderRuntime {
            provider,
            platform,
            endpoint_path: endpoint_path_for_provider(provider),
            auth_token: api_key,
            default_model: self.default_model,
            transport,
        })
    }
}

#[derive(Debug, Clone)]
struct ProviderClient {
    runtime: Arc<ProviderRuntime>,
}

impl ProviderClient {
    fn new(runtime: ProviderRuntime) -> Self {
        Self {
            runtime: Arc::new(runtime),
        }
    }

    fn messages(&self) -> MessagesApi<'_> {
        MessagesApi { client: self }
    }

    async fn create(&self, input: MessageCreateInput) -> Result<Response, RuntimeError> {
        self.create_with_meta(input)
            .await
            .map(|(response, _)| response)
    }

    async fn create_with_meta(
        &self,
        input: MessageCreateInput,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let request =
            input.into_request_with_options(self.runtime.default_model.as_deref(), false)?;
        self.send_with_meta(request).await
    }

    async fn send(&self, request: Request) -> Result<Response, RuntimeError> {
        self.send_with_meta(request)
            .await
            .map(|(response, _)| response)
    }

    async fn send_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let attempt = self
            .runtime
            .execute_attempt(request, None, BTreeMap::new())
            .await;

        match attempt {
            ProviderAttemptOutcome::Success { response, meta } => Ok((
                response,
                ResponseMeta {
                    selected_provider: meta.provider,
                    selected_model: meta.model.clone(),
                    status_code: meta.status_code,
                    request_id: meta.request_id.clone(),
                    attempts: vec![meta],
                },
            )),
            ProviderAttemptOutcome::Failure { error, .. } => Err(error),
        }
    }
}

#[derive(Debug, Clone)]
struct ProviderRuntime {
    provider: ProviderId,
    platform: PlatformConfig,
    endpoint_path: &'static str,
    auth_token: String,
    default_model: Option<String>,
    transport: HttpTransport,
}

enum ProviderAttemptOutcome {
    Success {
        response: Response,
        meta: AttemptMeta,
    },
    Failure {
        error: RuntimeError,
        meta: AttemptMeta,
    },
}

impl ProviderRuntime {
    async fn execute_attempt(
        &self,
        mut request: Request,
        model_override: Option<&str>,
        metadata: BTreeMap<String, String>,
    ) -> ProviderAttemptOutcome {
        let selected_model = match self.resolve_model(&request.model_id, model_override) {
            Ok(model) => model,
            Err(error) => {
                return ProviderAttemptOutcome::Failure {
                    meta: AttemptMeta {
                        provider: self.provider,
                        model: "<unset-model>".to_string(),
                        success: false,
                        status_code: None,
                        request_id: None,
                        error_kind: Some(error.kind),
                        error_message: Some(error.message.clone()),
                    },
                    error,
                };
            }
        };
        request.model_id = selected_model.clone();

        let adapter_context = AdapterContext {
            metadata,
            auth_token: Some(AuthCredentials::Token(self.auth_token.clone())),
        };
        let url = join_url(&self.platform.base_url, self.endpoint_path);

        let provider_response = match self.provider {
            ProviderId::OpenAi => {
                self.execute_openai_attempt(&request, &url, &adapter_context)
                    .await
            }
            ProviderId::Anthropic => {
                self.execute_anthropic_attempt(&request, &url, &adapter_context)
                    .await
            }
            ProviderId::OpenRouter => {
                self.execute_openrouter_attempt(&request, &url, &adapter_context)
                    .await
            }
        };

        match provider_response {
            Ok((response, http_response)) => ProviderAttemptOutcome::Success {
                meta: AttemptMeta {
                    provider: self.provider,
                    model: selected_model,
                    success: true,
                    status_code: Some(http_response.status.as_u16()),
                    request_id: http_response.request_id.clone(),
                    error_kind: None,
                    error_message: None,
                },
                response,
            },
            Err(error) => ProviderAttemptOutcome::Failure {
                meta: AttemptMeta {
                    provider: self.provider,
                    model: selected_model,
                    success: false,
                    status_code: error.status_code,
                    request_id: error.request_id.clone(),
                    error_kind: Some(error.kind),
                    error_message: Some(error.message.clone()),
                },
                error,
            },
        }
    }

    fn resolve_model(
        &self,
        request_model: &str,
        model_override: Option<&str>,
    ) -> Result<String, RuntimeError> {
        let trimmed_override = model_override.and_then(trimmed_non_empty);
        if let Some(model) = trimmed_override {
            return Ok(model.to_string());
        }

        if let Some(model) = trimmed_non_empty(request_model) {
            return Ok(model.to_string());
        }

        if let Some(default_model) = self.default_model.as_deref().and_then(trimmed_non_empty) {
            return Ok(default_model.to_string());
        }

        Err(RuntimeError::configuration(format!(
            "no model available for provider {:?}; set a default model or pass one per request",
            self.provider
        )))
    }

    async fn execute_openai_attempt(
        &self,
        request: &Request,
        url: &str,
        adapter_context: &AdapterContext,
    ) -> Result<(Response, HttpJsonResponse), RuntimeError> {
        let translator = OpenAiTranslator;
        let encoded = translator
            .encode_request(request)
            .map_err(|error| RuntimeError::from_adapter(error.into()))?;
        let provider_response = self
            .transport
            .post_json_value(&self.platform, url, &encoded.body, adapter_context)
            .await
            .map_err(|error| RuntimeError::from_transport(self.provider, error))?;
        let envelope = OpenAiDecodeEnvelope {
            body: provider_response.body.clone(),
            requested_response_format: request.response_format.clone(),
        };
        let mut response = translator.decode_request(&envelope).map_err(|error| {
            self.runtime_error_from_adapter(error.into(), Some(&provider_response))
        })?;
        prepend_encode_warnings(&mut response, encoded.warnings);
        Ok((response, provider_response))
    }

    async fn execute_anthropic_attempt(
        &self,
        request: &Request,
        url: &str,
        adapter_context: &AdapterContext,
    ) -> Result<(Response, HttpJsonResponse), RuntimeError> {
        let translator = AnthropicTranslator;
        let encoded = translator
            .encode_request(request)
            .map_err(|error| RuntimeError::from_adapter(error.into()))?;
        let provider_response = self
            .transport
            .post_json_value(&self.platform, url, &encoded.body, adapter_context)
            .await
            .map_err(|error| RuntimeError::from_transport(self.provider, error))?;
        let envelope = AnthropicDecodeEnvelope {
            body: provider_response.body.clone(),
            requested_response_format: request.response_format.clone(),
        };
        let mut response = translator.decode_request(&envelope).map_err(|error| {
            self.runtime_error_from_adapter(error.into(), Some(&provider_response))
        })?;
        prepend_encode_warnings(&mut response, encoded.warnings);
        Ok((response, provider_response))
    }

    async fn execute_openrouter_attempt(
        &self,
        request: &Request,
        url: &str,
        adapter_context: &AdapterContext,
    ) -> Result<(Response, HttpJsonResponse), RuntimeError> {
        let translator = OpenRouterTranslator::default();
        let encoded = translator
            .encode_request(request)
            .map_err(|error| RuntimeError::from_adapter(error.into()))?;
        let provider_response = self
            .transport
            .post_json_value(&self.platform, url, &encoded.body, adapter_context)
            .await
            .map_err(|error| RuntimeError::from_transport(self.provider, error))?;
        let envelope = OpenAiDecodeEnvelope {
            body: provider_response.body.clone(),
            requested_response_format: request.response_format.clone(),
        };
        let mut response = translator.decode_request(&envelope).map_err(|error| {
            self.runtime_error_from_adapter(error.into(), Some(&provider_response))
        })?;
        prepend_encode_warnings(&mut response, encoded.warnings);
        Ok((response, provider_response))
    }

    fn runtime_error_from_adapter(
        &self,
        mut adapter_error: AdapterError,
        response: Option<&HttpJsonResponse>,
    ) -> RuntimeError {
        if let Some(response) = response {
            if adapter_error.status_code.is_none() {
                adapter_error.status_code = Some(response.status.as_u16());
            }
            if adapter_error.request_id.is_none() {
                adapter_error.request_id = response.request_id.clone();
            }
            if adapter_error.provider_code.is_none() {
                adapter_error.provider_code = extract_provider_code(&response.body);
            }
        }
        RuntimeError::from_adapter(adapter_error)
    }
}

fn prepend_encode_warnings(
    response: &mut Response,
    mut encode_warnings: Vec<agent_core::types::RuntimeWarning>,
) {
    if encode_warnings.is_empty() {
        return;
    }
    encode_warnings.append(&mut response.warnings);
    response.warnings = encode_warnings;
}

fn extract_provider_code(body: &serde_json::Value) -> Option<String> {
    body.get("error")
        .and_then(serde_json::Value::as_object)
        .and_then(|error| error.get("code").or_else(|| error.get("type")))
        .and_then(value_to_string)
}

fn value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) if !value.trim().is_empty() => {
            Some(value.trim().to_string())
        }
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn default_base_url_for_provider(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::OpenAi => OPENAI_BASE_URL,
        ProviderId::Anthropic => ANTHROPIC_BASE_URL,
        ProviderId::OpenRouter => OPENROUTER_BASE_URL,
    }
}

fn endpoint_path_for_provider(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::OpenAi => OPENAI_ENDPOINT_PATH,
        ProviderId::Anthropic => ANTHROPIC_ENDPOINT_PATH,
        ProviderId::OpenRouter => OPENROUTER_ENDPOINT_PATH,
    }
}

fn platform_config_for_provider(
    provider: ProviderId,
    base_url: String,
) -> Result<PlatformConfig, RuntimeError> {
    let request_id_header = match provider {
        ProviderId::OpenAi | ProviderId::OpenRouter => HeaderName::from_static("x-request-id"),
        ProviderId::Anthropic => HeaderName::from_static("request-id"),
    };

    let mut default_headers = HeaderMap::new();
    if provider == ProviderId::Anthropic {
        default_headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
    }

    let auth_style = match provider {
        ProviderId::OpenAi | ProviderId::OpenRouter => AuthStyle::Bearer,
        ProviderId::Anthropic => AuthStyle::ApiKeyHeader(HeaderName::from_static("x-api-key")),
    };

    let protocol = match provider {
        ProviderId::Anthropic => ProtocolKind::Anthropic,
        ProviderId::OpenAi | ProviderId::OpenRouter => ProtocolKind::OpenAI,
    };

    let trimmed_base_url = base_url.trim().to_string();
    if trimmed_base_url.is_empty() {
        return Err(RuntimeError::configuration(format!(
            "base_url is empty for provider {provider:?}"
        )));
    }

    Ok(PlatformConfig {
        protocol,
        base_url: trimmed_base_url,
        auth_style,
        request_id_header,
        default_headers,
    })
}

fn map_adapter_error_kind(kind: AdapterErrorKind) -> RuntimeErrorKind {
    match kind {
        AdapterErrorKind::Validation => RuntimeErrorKind::Validation,
        AdapterErrorKind::Encode => RuntimeErrorKind::Encode,
        AdapterErrorKind::Decode => RuntimeErrorKind::Decode,
        AdapterErrorKind::ProtocolViolation => RuntimeErrorKind::ProtocolViolation,
        AdapterErrorKind::UnsupportedFeature => RuntimeErrorKind::UnsupportedFeature,
        AdapterErrorKind::Upstream => RuntimeErrorKind::Upstream,
        AdapterErrorKind::Transport => RuntimeErrorKind::Transport,
    }
}

fn map_adapter_protocol(protocol: AdapterProtocol) -> ProviderId {
    match protocol {
        AdapterProtocol::OpenAI => ProviderId::OpenAi,
        AdapterProtocol::Anthropic => ProviderId::Anthropic,
        AdapterProtocol::OpenRouter => ProviderId::OpenRouter,
    }
}

fn read_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
}

fn require_env(key: &str) -> Result<String, RuntimeError> {
    read_env(key)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RuntimeError::configuration(format!("missing required env var {key}")))
}

fn join_url(base_url: &str, endpoint_path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        endpoint_path.trim_start_matches('/')
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn message_input_from_str_creates_user_message() {
        let input = MessageCreateInput::from("hello");
        assert_eq!(input.messages.len(), 1);
        assert_eq!(input.messages[0].role, MessageRole::User);
    }

    #[test]
    fn fallback_policy_matches_transport_or_retryable_status() {
        let policy = FallbackPolicy::default();

        let transport_error = RuntimeError {
            kind: RuntimeErrorKind::Transport,
            message: "transport".to_string(),
            provider: Some(ProviderId::OpenAi),
            status_code: None,
            request_id: None,
            provider_code: None,
            source: None,
        };
        assert!(policy.should_fallback(&transport_error));

        let rate_limit_error = RuntimeError {
            kind: RuntimeErrorKind::Upstream,
            message: "rate limited".to_string(),
            provider: Some(ProviderId::OpenAi),
            status_code: Some(429),
            request_id: None,
            provider_code: None,
            source: None,
        };
        assert!(policy.should_fallback(&rate_limit_error));
    }

    #[test]
    fn router_requires_explicit_target_without_policy() {
        let toolkit = AgentToolkit {
            clients: HashMap::new(),
        };
        let error = toolkit
            .resolve_targets(&SendOptions::default())
            .expect_err("target resolution should fail");
        assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
    }

    #[test]
    fn message_input_uses_default_model_when_missing() {
        let request = MessageCreateInput::from("hello")
            .into_request_with_options(Some("default-model"), false)
            .expect("default model should be used");
        assert_eq!(request.model_id, "default-model");
    }

    #[test]
    fn message_input_allows_empty_model_for_router_path() {
        let request = MessageCreateInput::from("hello")
            .into_request_with_options(None, true)
            .expect("empty model should be allowed for router path");
        assert!(request.model_id.is_empty());
    }
}
