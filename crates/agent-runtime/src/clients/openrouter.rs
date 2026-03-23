use crate::clients::BaseClientBuilder;
use crate::clients::common::{ClientEnv, impl_provider_client};
use crate::provider::ProviderClient;
use crate::{
    AttemptSpec, ExecutionOptions, MessageCreateInput, MessageResponseStream, RuntimeError,
};
use agent_core::{
    FamilyOptions, NativeOptions, OpenAiCompatibleOptions, OpenRouterOptions, ProviderKind,
    ProviderOptions, Response, ResponseMode, TaskRequest,
};

const OPENROUTER_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
const OPENROUTER_BASE_URL_ENV: &str = "OPENROUTER_BASE_URL";
const OPENROUTER_MODEL_ENV: &str = "OPENROUTER_MODEL";

/// Direct client for OpenRouter requests.
#[derive(Debug, Clone)]
pub struct OpenRouterClient {
    inner: ProviderClient,
}

/// Builder for [`OpenRouterClient`].
#[derive(Debug, Clone, Default)]
pub struct OpenRouterClientBuilder {
    inner: BaseClientBuilder,
}

impl_provider_client!(
    client = OpenRouterClient,
    builder = OpenRouterClientBuilder,
    provider = ProviderKind::OpenRouter,
    constructor = openrouter,
    env = ClientEnv::new(
        OPENROUTER_API_KEY_ENV,
        OPENROUTER_BASE_URL_ENV,
        OPENROUTER_MODEL_ENV
    )
);

impl OpenRouterClient {
    fn openrouter_attempt(
        &self,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
    ) -> AttemptSpec {
        let mut attempt = self.inner.default_attempt();
        attempt.target.model = model;
        attempt.execution.native = Some(NativeOptions {
            family: family.map(FamilyOptions::OpenAiCompatible),
            provider: provider.map(|options| ProviderOptions::OpenRouter(Box::new(options))),
        });
        attempt
    }

    /// Executes a direct OpenRouter request with typed native options.
    ///
    /// `input` stays semantic-only (`messages`, tools, tool choice, response
    /// format). OpenAI-compatible family controls such as
    /// `parallel_tool_calls`, `reasoning`, `temperature`, `top_p`, and
    /// `max_output_tokens` belong in `family`.
    ///
    /// Router-native controls such as `provider_preferences`, metadata,
    /// plugins, penalties, `top_k`, `top_logprobs`, `text.verbosity`,
    /// `fallback_models`, `modalities`, `image_config`, and the approved
    /// parameter-doc-backed fields `max_tokens`, `stop`, `seed`, `logit_bias`,
    /// and `logprobs` belong in `provider`. `fallback_models` is encoded to the
    /// OpenRouter wire `models` array, and non-doc-backed `route` and `debug`
    /// fields are intentionally not part of [`OpenRouterOptions`].
    ///
    /// Validation follows ownership in the current implementation: the
    /// OpenAI-compatible family codec validates `family`, and the OpenRouter
    /// provider refinement validates `provider`.
    pub async fn create_with_openrouter_options(
        &self,
        input: impl Into<MessageCreateInput>,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
    ) -> Result<Response, RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_with_openrouter_options(
            task,
            model,
            family,
            provider,
            ExecutionOptions::default(),
        )
        .await
    }

    /// Executes a semantic [`TaskRequest`] on the direct OpenRouter path with
    /// separate family-scoped and provider-scoped native options.
    pub async fn execute_with_openrouter_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.inner
            .execute_on_attempt(
                task,
                self.openrouter_attempt(model, family, provider),
                execution,
            )
            .await
    }

    /// Executes a streaming direct OpenRouter request with typed native
    /// options.
    ///
    /// Ownership and validation rules are the same as
    /// [`Self::create_with_openrouter_options`].
    pub async fn create_stream_with_openrouter_options(
        &self,
        input: impl Into<MessageCreateInput>,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
    ) -> Result<MessageResponseStream, RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_stream_with_openrouter_options(
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

    /// Executes a streaming semantic [`TaskRequest`] on the direct OpenRouter
    /// path with separate family-scoped and provider-scoped native options.
    pub async fn execute_stream_with_openrouter_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.inner
            .execute_stream_on_attempt(
                task,
                self.openrouter_attempt(model, family, provider),
                execution,
            )
            .await
    }
}
