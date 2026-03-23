use crate::clients::base_client_builder::BaseClientBuilder;
use crate::clients::common::{ClientEnv, impl_provider_client};
use crate::provider::ProviderClient;
use crate::{
    AttemptSpec, ExecutionOptions, MessageCreateInput, MessageResponseStream, RuntimeError,
};
use agent_core::{
    FamilyOptions, NativeOptions, OpenAiCompatibleOptions, OpenAiOptions, ProviderKind,
    ProviderOptions, Response, ResponseMode, TaskRequest,
};

const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const OPENAI_BASE_URL_ENV: &str = "OPENAI_BASE_URL";
const OPENAI_MODEL_ENV: &str = "OPENAI_MODEL";

/// Direct client for the OpenAI-compatible runtime path.
#[derive(Debug, Clone)]
pub struct OpenAiClient {
    inner: ProviderClient,
}

/// Builder for [`OpenAiClient`].
#[derive(Debug, Clone, Default)]
pub struct OpenAiClientBuilder {
    inner: BaseClientBuilder,
}

impl_provider_client!(
    client = OpenAiClient,
    builder = OpenAiClientBuilder,
    provider = ProviderKind::OpenAi,
    constructor = openai,
    env = ClientEnv::new(OPENAI_API_KEY_ENV, OPENAI_BASE_URL_ENV, OPENAI_MODEL_ENV)
);

impl OpenAiClient {
    fn openai_attempt(
        &self,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
    ) -> AttemptSpec {
        let mut attempt = self.inner.default_attempt();
        attempt.target.model = model;
        attempt.execution.native = Some(NativeOptions {
            family: family.map(FamilyOptions::OpenAiCompatible),
            provider: provider.map(ProviderOptions::OpenAi),
        });
        attempt
    }

    /// Executes a direct OpenAI request with typed native options.
    ///
    /// `input` stays semantic-only (`messages`, tools, tool choice, response format).
    /// Request controls such as `temperature`, `top_p`, and `max_output_tokens`
    /// must be passed in `family`, and OpenAI-specific controls such as
    /// `metadata`, `service_tier`, `store`, `prompt_cache_key`,
    /// `prompt_cache_retention`, `truncation`, `text.verbosity`, and
    /// `safety_identifier` must be passed in `provider`.
    pub async fn create_with_openai_options(
        &self,
        input: impl Into<MessageCreateInput>,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
    ) -> Result<Response, RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_with_openai_options(task, model, family, provider, ExecutionOptions::default())
            .await
    }

    pub async fn execute_with_openai_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.inner
            .execute_on_attempt(
                task,
                self.openai_attempt(model, family, provider),
                execution,
            )
            .await
    }

    pub async fn create_stream_with_openai_options(
        &self,
        input: impl Into<MessageCreateInput>,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
    ) -> Result<MessageResponseStream, RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_stream_with_openai_options(
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

    pub async fn execute_stream_with_openai_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.inner
            .execute_stream_on_attempt(
                task,
                self.openai_attempt(model, family, provider),
                execution,
            )
            .await
    }
}
