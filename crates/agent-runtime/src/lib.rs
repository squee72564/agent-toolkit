use std::collections::{BTreeMap, HashMap};
use std::error::Error as StdError;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

use agent_core::types::{
    AdapterContext, AuthCredentials, Message, PlatformConfig, ProviderId, Request, Response,
    ResponseFormat, ToolChoice, ToolDefinition,
};
use agent_providers::adapter::{ProviderAdapter, adapter_for};
use agent_providers::error::{AdapterError, AdapterErrorKind};
use agent_transport::{HttpJsonResponse, HttpTransport, RetryPolicy, TransportError};

const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const OPENAI_BASE_URL_ENV: &str = "OPENAI_BASE_URL";
const OPENAI_MODEL_ENV: &str = "OPENAI_MODEL";
const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const ANTHROPIC_BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
const ANTHROPIC_MODEL_ENV: &str = "ANTHROPIC_MODEL";
const OPENROUTER_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
const OPENROUTER_BASE_URL_ENV: &str = "OPENROUTER_BASE_URL";
const OPENROUTER_MODEL_ENV: &str = "OPENROUTER_MODEL";

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FallbackMode {
    LegacyOnly,
    RulesOnly,
    #[default]
    LegacyOrRules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackAction {
    RetryNextTarget,
    Stop,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FallbackMatch {
    pub error_kinds: Vec<RuntimeErrorKind>,
    pub status_codes: Vec<u16>,
    pub provider_codes: Vec<String>,
    pub providers: Vec<ProviderId>,
}

impl FallbackMatch {
    fn matches(&self, error: &RuntimeError) -> bool {
        if !self.error_kinds.is_empty() && !self.error_kinds.contains(&error.kind) {
            return false;
        }

        if !self.status_codes.is_empty() {
            let Some(status_code) = error.status_code else {
                return false;
            };
            if !self.status_codes.contains(&status_code) {
                return false;
            }
        }

        if !self.provider_codes.is_empty() {
            let Some(provider_code) = error.provider_code.as_deref().and_then(trimmed_non_empty)
            else {
                return false;
            };
            if !self
                .provider_codes
                .iter()
                .filter_map(|code| trimmed_non_empty(code))
                .any(|code| code == provider_code)
            {
                return false;
            }
        }

        if !self.providers.is_empty() {
            let Some(provider) = error.provider else {
                return false;
            };
            if !self.providers.contains(&provider) {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackRule {
    pub when: FallbackMatch,
    pub action: FallbackAction,
}

impl FallbackRule {
    pub fn retry_on_status(status_code: u16) -> Self {
        Self {
            when: FallbackMatch {
                status_codes: vec![status_code],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    pub fn retry_on_kind(kind: RuntimeErrorKind) -> Self {
        Self {
            when: FallbackMatch {
                error_kinds: vec![kind],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    pub fn retry_on_provider_code(provider_code: impl Into<String>) -> Self {
        Self {
            when: FallbackMatch {
                provider_codes: vec![provider_code.into()],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    pub fn stop_on_kind(kind: RuntimeErrorKind) -> Self {
        Self {
            when: FallbackMatch {
                error_kinds: vec![kind],
                ..FallbackMatch::default()
            },
            action: FallbackAction::Stop,
        }
    }

    pub fn for_provider(mut self, provider: ProviderId) -> Self {
        if !self.when.providers.contains(&provider) {
            self.when.providers.push(provider);
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackPolicy {
    pub targets: Vec<Target>,
    pub retry_on_status_codes: Vec<u16>,
    pub retry_on_transport_error: bool,
    pub rules: Vec<FallbackRule>,
    pub mode: FallbackMode,
}

impl FallbackPolicy {
    pub fn new(targets: Vec<Target>) -> Self {
        Self {
            targets,
            ..Self::default()
        }
    }

    pub fn with_mode(mut self, mode: FallbackMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_rule(mut self, rule: FallbackRule) -> Self {
        self.rules.push(rule);
        self
    }

    fn should_fallback(&self, error: &RuntimeError) -> bool {
        let legacy_decision = self.should_fallback_legacy(error);
        let rules_decision = self.should_fallback_rules(error);

        match self.mode {
            FallbackMode::LegacyOnly => legacy_decision,
            FallbackMode::RulesOnly => rules_decision,
            FallbackMode::LegacyOrRules => legacy_decision || rules_decision,
        }
    }

    fn should_fallback_legacy(&self, error: &RuntimeError) -> bool {
        if self.retry_on_transport_error && error.kind == RuntimeErrorKind::Transport {
            return true;
        }

        if let Some(status_code) = error.status_code {
            return self.retry_on_status_codes.contains(&status_code);
        }

        false
    }

    fn should_fallback_rules(&self, error: &RuntimeError) -> bool {
        for rule in &self.rules {
            if rule.when.matches(error) {
                return matches!(rule.action, FallbackAction::RetryNextTarget);
            }
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
            rules: Vec::new(),
            mode: FallbackMode::LegacyOrRules,
        }
    }
}

pub trait RuntimeObserver: Send + Sync {
    fn on_request_start(&self, _event: &RequestStartEvent) {}
    fn on_attempt_start(&self, _event: &AttemptStartEvent) {}
    fn on_attempt_success(&self, _event: &AttemptSuccessEvent) {}
    fn on_attempt_failure(&self, _event: &AttemptFailureEvent) {}
    fn on_request_end(&self, _event: &RequestEndEvent) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestStartEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
    pub first_target: Option<ProviderId>,
    pub resolved_target_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptStartEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptSuccessEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptFailureEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
    pub error_kind: Option<RuntimeErrorKind>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestEndEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
    pub status_code: Option<u16>,
    pub error_kind: Option<RuntimeErrorKind>,
    pub error_message: Option<String>,
}

#[derive(Clone, Default)]
pub struct SendOptions {
    pub target: Option<Target>,
    pub fallback_policy: Option<FallbackPolicy>,
    pub metadata: BTreeMap<String, String>,
    pub observer: Option<Arc<dyn RuntimeObserver>>,
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

    pub fn with_observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.observer = Some(observer);
        self
    }
}

impl std::fmt::Debug for SendOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendOptions")
            .field("target", &self.target)
            .field("fallback_policy", &self.fallback_policy)
            .field("metadata", &self.metadata)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
}

impl PartialEq for SendOptions {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
            && self.fallback_policy == other.fallback_policy
            && self.metadata == other.metadata
            && match (&self.observer, &other.observer) {
                (Some(lhs), Some(rhs)) => Arc::ptr_eq(lhs, rhs),
                (None, None) => true,
                _ => false,
            }
    }
}

impl Eq for SendOptions {}

fn resolve_observer_for_request<'a>(
    client_observer: Option<&'a Arc<dyn RuntimeObserver>>,
    toolkit_observer: Option<&'a Arc<dyn RuntimeObserver>>,
    send_observer: Option<&'a Arc<dyn RuntimeObserver>>,
) -> Option<&'a Arc<dyn RuntimeObserver>> {
    send_observer.or(toolkit_observer).or(client_observer)
}

fn safe_call_observer(
    observer: Option<&Arc<dyn RuntimeObserver>>,
    call: impl FnOnce(&dyn RuntimeObserver),
) {
    if let Some(observer) = observer {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            call(observer.as_ref());
        }));
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
        let provider = error.provider;

        Self {
            kind: map_adapter_error_kind(error.kind),
            message: error.message.clone(),
            provider: Some(provider),
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
enum MessagesPayload {
    Owned(Vec<Message>),
    Shared(Arc<Vec<Message>>),
}

impl Default for MessagesPayload {
    fn default() -> Self {
        Self::Owned(Vec::new())
    }
}

impl MessagesPayload {
    fn as_slice(&self) -> &[Message] {
        match self {
            Self::Owned(messages) => messages.as_slice(),
            Self::Shared(messages) => messages.as_slice(),
        }
    }

    fn into_vec(self) -> Vec<Message> {
        match self {
            Self::Owned(messages) => messages,
            Self::Shared(messages) => messages.as_ref().clone(),
        }
    }

    fn to_mut(&mut self) -> &mut Vec<Message> {
        if let Self::Shared(messages) = self {
            let cloned = messages.as_ref().clone();
            *self = Self::Owned(cloned);
        }

        match self {
            Self::Owned(messages) => messages,
            Self::Shared(_) => unreachable!("shared payload should materialize before mutation"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageCreateInput {
    pub model: Option<String>,
    messages: MessagesPayload,
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
        Self::new_owned(messages)
    }

    fn new_owned(messages: Vec<Message>) -> Self {
        Self {
            model: None,
            messages: MessagesPayload::Owned(messages),
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

    fn new_shared(messages: Arc<Vec<Message>>) -> Self {
        Self {
            model: None,
            messages: MessagesPayload::Shared(messages),
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

    pub fn messages(&self) -> &[Message] {
        self.messages.as_slice()
    }

    pub fn messages_mut(&mut self) -> &mut Vec<Message> {
        self.messages.to_mut()
    }

    pub fn into_messages(self) -> Vec<Message> {
        self.messages.into_vec()
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = tool_choice;
        self
    }

    pub fn with_response_format(mut self, response_format: ResponseFormat) -> Self {
        self.response_format = response_format;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    pub fn with_max_output_tokens(mut self, max_output_tokens: u32) -> Self {
        self.max_output_tokens = Some(max_output_tokens);
        self
    }

    pub fn with_stop<I, S>(mut self, stop: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.stop = stop.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_metadata(mut self, metadata: BTreeMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    fn into_request_with_options(
        self,
        default_model: Option<&str>,
        allow_empty_model: bool,
    ) -> Result<Request, RuntimeError> {
        let MessageCreateInput {
            model,
            messages,
            tools,
            tool_choice,
            response_format,
            temperature,
            top_p,
            max_output_tokens,
            stop,
            metadata,
        } = self;

        let messages = messages.into_vec();
        if messages.is_empty() {
            return Err(RuntimeError::configuration(
                "messages().create(...) requires at least one message",
            ));
        }

        let model_id = match (model, default_model) {
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
            messages,
            tools,
            tool_choice,
            response_format,
            temperature,
            top_p,
            max_output_tokens,
            stop,
            metadata,
        })
    }
}

impl From<String> for MessageCreateInput {
    fn from(text: String) -> Self {
        Self::new(vec![Message::user_text(text)])
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    messages: Arc<Vec<Message>>,
}

impl Conversation {
    /// Creates an empty conversation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use agent_runtime::Conversation;
    ///
    /// let conversation = Conversation::new();
    /// assert!(conversation.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a conversation from an existing message history.
    pub fn from_messages(messages: Vec<Message>) -> Self {
        Self {
            messages: Arc::new(messages),
        }
    }

    pub fn with_system_text(text: impl Into<String>) -> Self {
        Self::from_messages(vec![Message::system_text(text)])
    }

    pub fn len(&self) -> usize {
        self.messages.as_ref().len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.as_ref().is_empty()
    }

    pub fn messages(&self) -> &[Message] {
        self.messages.as_slice()
    }

    pub fn clone_messages(&self) -> Vec<Message> {
        self.messages.as_ref().clone()
    }

    pub fn push_message(&mut self, message: Message) {
        Arc::make_mut(&mut self.messages).push(message);
    }

    pub fn extend_messages<I>(&mut self, messages: I)
    where
        I: IntoIterator<Item = Message>,
    {
        Arc::make_mut(&mut self.messages).extend(messages);
    }

    pub fn push_user_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::user_text(text));
    }

    pub fn push_system_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::system_text(text));
    }

    pub fn push_assistant_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::assistant_text(text));
    }

    pub fn push_assistant_tool_call(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_json: serde_json::Value,
    ) {
        self.push_message(Message::assistant_tool_call(id, name, arguments_json));
    }

    pub fn push_tool_result_json(
        &mut self,
        tool_call_id: impl Into<String>,
        value: serde_json::Value,
    ) {
        self.push_message(Message::tool_result_json(tool_call_id, value));
    }

    pub fn push_tool_result_text(
        &mut self,
        tool_call_id: impl Into<String>,
        text: impl Into<String>,
    ) {
        self.push_message(Message::tool_result_text(tool_call_id, text));
    }

    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.messages).clear();
    }

    pub fn to_input(&self) -> MessageCreateInput {
        MessageCreateInput::new_shared(Arc::clone(&self.messages))
    }

    pub fn into_input(self) -> MessageCreateInput {
        match Arc::try_unwrap(self.messages) {
            Ok(messages) => MessageCreateInput::new_owned(messages),
            Err(messages) => MessageCreateInput::new_shared(messages),
        }
    }
}

impl From<Vec<Message>> for Conversation {
    fn from(messages: Vec<Message>) -> Self {
        Self::from_messages(messages)
    }
}

impl From<Conversation> for Vec<Message> {
    fn from(conversation: Conversation) -> Self {
        match Arc::try_unwrap(conversation.messages) {
            Ok(messages) => messages,
            Err(messages) => messages.as_ref().clone(),
        }
    }
}

impl From<Conversation> for MessageCreateInput {
    fn from(conversation: Conversation) -> Self {
        conversation.into_input()
    }
}

impl From<&Conversation> for MessageCreateInput {
    fn from(conversation: &Conversation) -> Self {
        conversation.to_input()
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

    pub fn observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.inner.observer = Some(observer);
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

    pub fn observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.inner.observer = Some(observer);
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

    pub fn observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.inner.observer = Some(observer);
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

#[derive(Clone)]
pub struct AgentToolkit {
    clients: HashMap<ProviderId, ProviderClient>,
    observer: Option<Arc<dyn RuntimeObserver>>,
}

impl std::fmt::Debug for AgentToolkit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentToolkit")
            .field("clients", &self.clients)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
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
        let request_started_at = std::time::Instant::now();
        let targets = self.resolve_targets(&options)?;
        let first_client_observer = targets
            .first()
            .and_then(|target| self.clients.get(&target.provider))
            .and_then(|client| client.runtime.observer.as_ref());
        let request_observer = resolve_observer_for_request(
            first_client_observer,
            self.observer.as_ref(),
            options.observer.as_ref(),
        );
        let request_start_event = RequestStartEvent {
            request_id: None,
            provider: targets.first().map(|target| target.provider),
            model: targets
                .first()
                .and_then(|target| event_model(target.model.as_deref(), &request.model_id)),
            target_index: None,
            attempt_index: None,
            elapsed: request_started_at.elapsed(),
            first_target: targets.first().map(|target| target.provider),
            resolved_target_count: targets.len(),
        };
        safe_call_observer(request_observer, |observer| {
            observer.on_request_start(&request_start_event)
        });

        let fallback_policy = options.fallback_policy.clone();
        let mut attempts = Vec::new();
        let mut last_error: Option<RuntimeError> = None;

        let request_model_id = request.model_id.clone();
        let mut request = Some(request);

        for (index, target) in targets.iter().enumerate() {
            let Some(client) = self.clients.get(&target.provider) else {
                let error = RuntimeError::target_resolution(format!(
                    "provider {:?} is not registered",
                    target.provider
                ));
                let request_end_event = RequestEndEvent {
                    request_id: error.request_id.clone(),
                    provider: Some(target.provider),
                    model: event_model(target.model.as_deref(), &request_model_id),
                    target_index: Some(index),
                    attempt_index: Some(index),
                    elapsed: request_started_at.elapsed(),
                    status_code: error.status_code,
                    error_kind: Some(error.kind),
                    error_message: Some(error.message.clone()),
                };
                safe_call_observer(request_observer, |runtime_observer| {
                    runtime_observer.on_request_end(&request_end_event);
                });
                return Err(error);
            };
            let observer = resolve_observer_for_request(
                client.runtime.observer.as_ref(),
                self.observer.as_ref(),
                options.observer.as_ref(),
            );
            let attempt_started_at = std::time::Instant::now();
            let attempt_start_event = AttemptStartEvent {
                request_id: None,
                provider: Some(target.provider),
                model: event_model(target.model.as_deref(), &request_model_id),
                target_index: Some(index),
                attempt_index: Some(index),
                elapsed: attempt_started_at.elapsed(),
            };
            safe_call_observer(observer, |runtime_observer| {
                runtime_observer.on_attempt_start(&attempt_start_event);
            });

            let is_last = index + 1 >= targets.len();
            let Some(attempt_request) = (if is_last {
                request.take()
            } else {
                request.as_ref().cloned()
            }) else {
                let error = RuntimeError::target_resolution(
                    "request state was exhausted before completing fallback attempts",
                );
                let request_end_event = RequestEndEvent {
                    request_id: error.request_id.clone(),
                    provider: Some(target.provider),
                    model: event_model(target.model.as_deref(), &request_model_id),
                    target_index: Some(index),
                    attempt_index: Some(index),
                    elapsed: request_started_at.elapsed(),
                    status_code: error.status_code,
                    error_kind: Some(error.kind),
                    error_message: Some(error.message.clone()),
                };
                safe_call_observer(request_observer, |runtime_observer| {
                    runtime_observer.on_request_end(&request_end_event);
                });
                return Err(error);
            };

            let attempt = client
                .runtime
                .execute_attempt(
                    attempt_request,
                    target.model.as_deref(),
                    options.metadata.clone(),
                )
                .await;

            match attempt {
                ProviderAttemptOutcome::Success { response, meta } => {
                    let attempt_success_event = AttemptSuccessEvent {
                        request_id: meta.request_id.clone(),
                        provider: Some(meta.provider),
                        model: Some(meta.model.clone()),
                        target_index: Some(index),
                        attempt_index: Some(index),
                        elapsed: attempt_started_at.elapsed(),
                        status_code: meta.status_code,
                    };
                    safe_call_observer(observer, |runtime_observer| {
                        runtime_observer.on_attempt_success(&attempt_success_event);
                    });

                    attempts.push(meta.clone());
                    let response_meta = ResponseMeta {
                        selected_provider: meta.provider,
                        selected_model: meta.model,
                        status_code: meta.status_code,
                        request_id: meta.request_id.clone(),
                        attempts,
                    };

                    let request_end_event = RequestEndEvent {
                        request_id: response_meta.request_id.clone(),
                        provider: Some(response_meta.selected_provider),
                        model: Some(response_meta.selected_model.clone()),
                        target_index: Some(index),
                        attempt_index: Some(index),
                        elapsed: request_started_at.elapsed(),
                        status_code: response_meta.status_code,
                        error_kind: None,
                        error_message: None,
                    };
                    safe_call_observer(observer, |runtime_observer| {
                        runtime_observer.on_request_end(&request_end_event);
                    });
                    return Ok((response, response_meta));
                }
                ProviderAttemptOutcome::Failure { error, meta } => {
                    let attempt_failure_event = AttemptFailureEvent {
                        request_id: meta.request_id.clone(),
                        provider: Some(meta.provider),
                        model: Some(meta.model.clone()),
                        target_index: Some(index),
                        attempt_index: Some(index),
                        elapsed: attempt_started_at.elapsed(),
                        error_kind: meta.error_kind,
                        error_message: meta.error_message.clone(),
                    };
                    safe_call_observer(observer, |runtime_observer| {
                        runtime_observer.on_attempt_failure(&attempt_failure_event);
                    });

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

        let result = match last_error {
            Some(error) if attempts.len() > 1 => Err(RuntimeError::fallback_exhausted(error)),
            Some(error) => Err(error),
            None => Err(RuntimeError::target_resolution(
                "no target providers were resolved for this request",
            )),
        };

        if let Err(error) = &result {
            let terminal_error = terminal_failure_error(error);
            let terminal_provider = terminal_error
                .provider
                .or_else(|| attempts.last().map(|attempt| attempt.provider));
            let terminal_observer = terminal_provider
                .and_then(|provider| self.clients.get(&provider))
                .and_then(|client| {
                    resolve_observer_for_request(
                        client.runtime.observer.as_ref(),
                        self.observer.as_ref(),
                        options.observer.as_ref(),
                    )
                });
            let terminal_index = attempts.len().checked_sub(1);
            let request_end_event = RequestEndEvent {
                request_id: terminal_error.request_id.clone(),
                provider: terminal_provider,
                model: attempts.last().map(|attempt| attempt.model.clone()),
                target_index: terminal_index,
                attempt_index: terminal_index,
                elapsed: request_started_at.elapsed(),
                status_code: terminal_error.status_code,
                error_kind: Some(terminal_error.kind),
                error_message: Some(terminal_error.message.clone()),
            };
            safe_call_observer(terminal_observer, |runtime_observer| {
                runtime_observer.on_request_end(&request_end_event);
            });
        }

        result
    }

    fn resolve_targets(&self, options: &SendOptions) -> Result<Vec<Target>, RuntimeError> {
        let mut targets = Vec::new();

        if let Some(primary_target) = &options.target {
            push_unique_target(&mut targets, primary_target.clone());
            if let Some(fallback_policy) = &options.fallback_policy {
                for target in &fallback_policy.targets {
                    push_unique_target(&mut targets, target.clone());
                }
            }
        } else if let Some(fallback_policy) = &options.fallback_policy {
            if fallback_policy.targets.is_empty() {
                return Err(RuntimeError::target_resolution(
                    "fallback policy requires at least one target",
                ));
            }
            for target in &fallback_policy.targets {
                push_unique_target(&mut targets, target.clone());
            }
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

#[derive(Clone, Default)]
pub struct AgentToolkitBuilder {
    openai: Option<ProviderConfig>,
    anthropic: Option<ProviderConfig>,
    openrouter: Option<ProviderConfig>,
    observer: Option<Arc<dyn RuntimeObserver>>,
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

    pub fn observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn build(self) -> Result<AgentToolkit, RuntimeError> {
        let AgentToolkitBuilder {
            openai,
            anthropic,
            openrouter,
            observer,
        } = self;
        let mut clients = HashMap::new();

        if let Some(config) = openai {
            let mut runtime_builder = BaseClientBuilder::from_provider_config(config);
            runtime_builder.observer = observer.clone();
            let runtime = runtime_builder.build_runtime(ProviderId::OpenAi)?;
            clients.insert(ProviderId::OpenAi, ProviderClient::new(runtime));
        }
        if let Some(config) = anthropic {
            let mut runtime_builder = BaseClientBuilder::from_provider_config(config);
            runtime_builder.observer = observer.clone();
            let runtime = runtime_builder.build_runtime(ProviderId::Anthropic)?;
            clients.insert(ProviderId::Anthropic, ProviderClient::new(runtime));
        }
        if let Some(config) = openrouter {
            let mut runtime_builder = BaseClientBuilder::from_provider_config(config);
            runtime_builder.observer = observer.clone();
            let runtime = runtime_builder.build_runtime(ProviderId::OpenRouter)?;
            clients.insert(ProviderId::OpenRouter, ProviderClient::new(runtime));
        }

        if clients.is_empty() {
            return Err(RuntimeError::configuration(
                "at least one provider must be configured",
            ));
        }

        Ok(AgentToolkit { clients, observer })
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

#[derive(Clone, Default)]
struct BaseClientBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    default_model: Option<String>,
    retry_policy: Option<RetryPolicy>,
    timeout: Option<Duration>,
    client: Option<reqwest::Client>,
    observer: Option<Arc<dyn RuntimeObserver>>,
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
            observer: None,
        }
    }

    fn build_runtime(self, provider: ProviderId) -> Result<ProviderRuntime, RuntimeError> {
        let adapter = adapter_for(provider);
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
            .unwrap_or_else(|| adapter.default_base_url().to_string());
        let platform = adapter
            .platform_config(base_url)
            .map_err(|error| RuntimeError::configuration(error.message))?;

        Ok(ProviderRuntime {
            provider,
            adapter,
            platform,
            auth_token: api_key,
            default_model: self.default_model,
            transport,
            observer: self.observer,
        })
    }
}

impl std::fmt::Debug for BaseClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BaseClientBuilder")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("retry_policy", &self.retry_policy)
            .field("timeout", &self.timeout)
            .field("client", &self.client.as_ref().map(|_| "configured"))
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
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
        let request_started_at = std::time::Instant::now();
        let observer = resolve_observer_for_request(self.runtime.observer.as_ref(), None, None);
        let request_start_event = RequestStartEvent {
            request_id: None,
            provider: Some(self.runtime.provider),
            model: if request.model_id.is_empty() {
                None
            } else {
                Some(request.model_id.clone())
            },
            target_index: None,
            attempt_index: None,
            elapsed: request_started_at.elapsed(),
            first_target: Some(self.runtime.provider),
            resolved_target_count: 1,
        };
        safe_call_observer(observer, |runtime_observer| {
            runtime_observer.on_request_start(&request_start_event);
        });
        let attempt_started_at = std::time::Instant::now();
        let attempt_start_event = AttemptStartEvent {
            request_id: None,
            provider: Some(self.runtime.provider),
            model: if request.model_id.is_empty() {
                None
            } else {
                Some(request.model_id.clone())
            },
            target_index: Some(0),
            attempt_index: Some(0),
            elapsed: attempt_started_at.elapsed(),
        };
        safe_call_observer(observer, |runtime_observer| {
            runtime_observer.on_attempt_start(&attempt_start_event);
        });

        let attempt = self
            .runtime
            .execute_attempt(request, None, BTreeMap::new())
            .await;

        match attempt {
            ProviderAttemptOutcome::Success { response, meta } => {
                let attempt_success_event = AttemptSuccessEvent {
                    request_id: meta.request_id.clone(),
                    provider: Some(meta.provider),
                    model: Some(meta.model.clone()),
                    target_index: Some(0),
                    attempt_index: Some(0),
                    elapsed: attempt_started_at.elapsed(),
                    status_code: meta.status_code,
                };
                safe_call_observer(observer, |runtime_observer| {
                    runtime_observer.on_attempt_success(&attempt_success_event);
                });
                let response_meta = ResponseMeta {
                    selected_provider: meta.provider,
                    selected_model: meta.model.clone(),
                    status_code: meta.status_code,
                    request_id: meta.request_id.clone(),
                    attempts: vec![meta],
                };
                let request_end_event = RequestEndEvent {
                    request_id: response_meta.request_id.clone(),
                    provider: Some(response_meta.selected_provider),
                    model: Some(response_meta.selected_model.clone()),
                    target_index: Some(0),
                    attempt_index: Some(0),
                    elapsed: request_started_at.elapsed(),
                    status_code: response_meta.status_code,
                    error_kind: None,
                    error_message: None,
                };
                safe_call_observer(observer, |runtime_observer| {
                    runtime_observer.on_request_end(&request_end_event);
                });

                Ok((response, response_meta))
            }
            ProviderAttemptOutcome::Failure { error, meta } => {
                let attempt_failure_event = AttemptFailureEvent {
                    request_id: meta.request_id.clone(),
                    provider: Some(meta.provider),
                    model: Some(meta.model.clone()),
                    target_index: Some(0),
                    attempt_index: Some(0),
                    elapsed: attempt_started_at.elapsed(),
                    error_kind: meta.error_kind,
                    error_message: meta.error_message,
                };
                safe_call_observer(observer, |runtime_observer| {
                    runtime_observer.on_attempt_failure(&attempt_failure_event);
                });

                let terminal_error = terminal_failure_error(&error);
                let request_end_event = RequestEndEvent {
                    request_id: terminal_error.request_id.clone(),
                    provider: terminal_error.provider,
                    model: Some(meta.model),
                    target_index: Some(0),
                    attempt_index: Some(0),
                    elapsed: request_started_at.elapsed(),
                    status_code: terminal_error.status_code,
                    error_kind: Some(terminal_error.kind),
                    error_message: Some(terminal_error.message.clone()),
                };
                safe_call_observer(observer, |runtime_observer| {
                    runtime_observer.on_request_end(&request_end_event);
                });

                Err(error)
            }
        }
    }
}

#[derive(Clone)]
struct ProviderRuntime {
    provider: ProviderId,
    adapter: &'static dyn ProviderAdapter,
    platform: PlatformConfig,
    auth_token: String,
    default_model: Option<String>,
    transport: HttpTransport,
    observer: Option<Arc<dyn RuntimeObserver>>,
}

impl std::fmt::Debug for ProviderRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRuntime")
            .field("provider", &self.provider)
            .field("platform", &self.platform)
            .field("auth_token", &"<redacted>")
            .field("default_model", &self.default_model)
            .field("transport", &self.transport)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
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
        let url = join_url(&self.platform.base_url, self.adapter.endpoint_path());

        let provider_response = self
            .execute_adapter_attempt(request, &url, &adapter_context)
            .await;

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

    async fn execute_adapter_attempt(
        &self,
        request: Request,
        url: &str,
        adapter_context: &AdapterContext,
    ) -> Result<(Response, HttpJsonResponse), RuntimeError> {
        let response_format = request.response_format.clone();
        let encoded = self
            .adapter
            .encode_request(request)
            .map_err(RuntimeError::from_adapter)?;
        let mut provider_response = self
            .transport
            .post_json_value(&self.platform, url, &encoded.body, adapter_context)
            .await
            .map_err(|error| RuntimeError::from_transport(self.provider, error))?;
        let provider_code = extract_provider_code(&provider_response.body);
        let response_body = std::mem::replace(&mut provider_response.body, serde_json::Value::Null);
        let mut response = self
            .adapter
            .decode_response(response_body, &response_format)
            .map_err(|mut error| {
                if error.provider_code.is_none() {
                    error.provider_code = provider_code;
                }
                self.runtime_error_from_adapter(error, Some(&provider_response))
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

fn event_model(target_model: Option<&str>, request_model: &str) -> Option<String> {
    target_model
        .and_then(trimmed_non_empty)
        .or_else(|| trimmed_non_empty(request_model))
        .map(ToString::to_string)
}

fn push_unique_target(targets: &mut Vec<Target>, target: Target) {
    if !targets.contains(&target) {
        targets.push(target);
    }
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

fn terminal_failure_error(error: &RuntimeError) -> &RuntimeError {
    if error.kind == RuntimeErrorKind::FallbackExhausted
        && let Some(source) = error.source_ref()
        && let Some(terminal_error) = source.downcast_ref::<RuntimeError>()
    {
        return terminal_error;
    }
    error
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
mod test;
