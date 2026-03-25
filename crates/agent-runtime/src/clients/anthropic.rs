use crate::clients::BaseClientBuilder;
use crate::clients::common::{ClientEnv, impl_provider_client};
use crate::provider::ProviderClient;
use crate::{
    AttemptSpec, ExecutionOptions, MessageCreateInput, MessageResponseStream, RuntimeError,
};
use agent_core::{
    AnthropicFamilyOptions, AnthropicOptions, FamilyOptions, NativeOptions, ProviderKind,
    ProviderOptions, Response, ResponseMode, TaskRequest,
};

const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const ANTHROPIC_BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
const ANTHROPIC_MODEL_ENV: &str = "ANTHROPIC_MODEL";

/// Direct client for Anthropic requests.
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    inner: ProviderClient,
}

/// Builder for [`AnthropicClient`].
#[derive(Debug, Clone, Default)]
pub struct AnthropicClientBuilder {
    inner: BaseClientBuilder,
}

impl_provider_client!(
    client = AnthropicClient,
    builder = AnthropicClientBuilder,
    provider = ProviderKind::Anthropic,
    constructor = anthropic,
    env = ClientEnv::new(
        ANTHROPIC_API_KEY_ENV,
        ANTHROPIC_BASE_URL_ENV,
        ANTHROPIC_MODEL_ENV
    )
);

impl AnthropicClient {
    fn anthropic_attempt(
        &self,
        model: Option<String>,
        family: Option<AnthropicFamilyOptions>,
        provider: Option<AnthropicOptions>,
    ) -> AttemptSpec {
        let mut attempt = self.inner.default_attempt();
        attempt.target.model = model;
        attempt.execution.native = Some(NativeOptions {
            family: family.map(FamilyOptions::Anthropic),
            provider: provider.map(ProviderOptions::Anthropic),
        });
        attempt
    }

    /// Executes a direct Anthropic request with typed native options.
    ///
    /// `input` stays semantic-only (`messages`, tools, tool choice, response
    /// format). Anthropic family controls such as `thinking` belong in
    /// `family`.
    ///
    /// Anthropic provider controls such as `temperature`, `top_p`,
    /// `max_tokens`, `top_k`, `stop_sequences`, `metadata_user_id`,
    /// map-style `metadata`, `output_config`, `service_tier`, `cache_control`,
    /// provider `tool_choice` overlays (for example
    /// `disable_parallel_tool_use`), and `inference_geo` belong in `provider`.
    ///
    /// `TaskRequest.tool_choice` remains the semantic source of truth.
    /// Provider `tool_choice` must be compatible with the semantic selection
    /// and is treated as an Anthropic-specific overlay.
    ///
    /// Validation follows ownership in the current implementation: the
    /// Anthropic family codec validates `family`, and the Anthropic provider
    /// refinement validates `provider`.
    pub async fn create_with_anthropic_options(
        &self,
        input: impl Into<MessageCreateInput>,
        model: Option<String>,
        family: Option<AnthropicFamilyOptions>,
        provider: Option<AnthropicOptions>,
    ) -> Result<Response, RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_with_anthropic_options(
            task,
            model,
            family,
            provider,
            ExecutionOptions::default(),
        )
        .await
    }

    /// Executes a semantic [`TaskRequest`] on the direct Anthropic path with
    /// separate family-scoped and provider-scoped native options.
    pub async fn execute_with_anthropic_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<AnthropicFamilyOptions>,
        provider: Option<AnthropicOptions>,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.inner
            .execute_on_attempt(
                task,
                self.anthropic_attempt(model, family, provider),
                execution,
            )
            .await
    }

    /// Executes a streaming direct Anthropic request with typed native
    /// options.
    ///
    /// Ownership and validation rules are the same as
    /// [`Self::create_with_anthropic_options`].
    pub async fn create_stream_with_anthropic_options(
        &self,
        input: impl Into<MessageCreateInput>,
        model: Option<String>,
        family: Option<AnthropicFamilyOptions>,
        provider: Option<AnthropicOptions>,
    ) -> Result<MessageResponseStream, RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_stream_with_anthropic_options(
            task,
            model,
            family,
            provider,
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
    }

    /// Executes a streaming semantic [`TaskRequest`] on the direct Anthropic
    /// path with separate family-scoped and provider-scoped native options.
    pub async fn execute_stream_with_anthropic_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<AnthropicFamilyOptions>,
        provider: Option<AnthropicOptions>,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.inner
            .execute_stream_on_attempt(
                task,
                self.anthropic_attempt(model, family, provider),
                execution,
            )
            .await
    }
}
