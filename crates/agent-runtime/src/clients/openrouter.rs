use crate::base_client_builder::BaseClientBuilder;
use crate::clients::common::{ClientEnv, impl_provider_client};
use crate::provider_client::ProviderClient;
use agent_core::{
    FamilyOptions, NativeOptions, OpenAiCompatibleOptions, OpenRouterOptions, ProviderKind,
    ProviderOptions, Response, TaskRequest,
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
    ) -> crate::AttemptSpec {
        let mut attempt = self.inner.default_attempt();
        attempt.target.model = model;
        attempt.execution.native = Some(NativeOptions {
            family: family.map(FamilyOptions::OpenAiCompatible),
            provider: provider.map(|options| ProviderOptions::OpenRouter(Box::new(options))),
        });
        attempt
    }

    pub async fn create_with_openrouter_options(
        &self,
        input: impl Into<crate::MessageCreateInput>,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
    ) -> Result<Response, crate::RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_with_openrouter_options(
            task,
            model,
            family,
            provider,
            crate::ExecutionOptions::default(),
        )
        .await
    }

    pub async fn execute_with_openrouter_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
        execution: crate::ExecutionOptions,
    ) -> Result<Response, crate::RuntimeError> {
        self.inner
            .execute_on_attempt(
                task,
                self.openrouter_attempt(model, family, provider),
                execution,
            )
            .await
    }

    pub async fn create_stream_with_openrouter_options(
        &self,
        input: impl Into<crate::MessageCreateInput>,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
    ) -> Result<crate::MessageResponseStream, crate::RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_stream_with_openrouter_options(
            task,
            model,
            family,
            provider,
            crate::ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..crate::ExecutionOptions::default()
            },
        )
        .await
    }

    pub async fn execute_stream_with_openrouter_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenRouterOptions>,
        execution: crate::ExecutionOptions,
    ) -> Result<crate::MessageResponseStream, crate::RuntimeError> {
        self.inner
            .execute_stream_on_attempt(
                task,
                self.openrouter_attempt(model, family, provider),
                execution,
            )
            .await
    }
}
