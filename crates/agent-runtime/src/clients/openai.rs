use crate::clients::base_client_builder::BaseClientBuilder;
use crate::clients::common::{ClientEnv, impl_provider_client};
use crate::provider::ProviderClient;
use agent_core::{
    FamilyOptions, NativeOptions, OpenAiCompatibleOptions, OpenAiOptions, ProviderKind,
    ProviderOptions, Response, TaskRequest,
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
    ) -> crate::AttemptSpec {
        let mut attempt = self.inner.default_attempt();
        attempt.target.model = model;
        attempt.execution.native = Some(NativeOptions {
            family: family.map(FamilyOptions::OpenAiCompatible),
            provider: provider.map(ProviderOptions::OpenAi),
        });
        attempt
    }

    pub async fn create_with_openai_options(
        &self,
        input: impl Into<crate::MessageCreateInput>,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
    ) -> Result<Response, crate::RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_with_openai_options(
            task,
            model,
            family,
            provider,
            crate::ExecutionOptions::default(),
        )
        .await
    }

    pub async fn execute_with_openai_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
        execution: crate::ExecutionOptions,
    ) -> Result<Response, crate::RuntimeError> {
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
        input: impl Into<crate::MessageCreateInput>,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
    ) -> Result<crate::MessageResponseStream, crate::RuntimeError> {
        let task = input.into().into_task_request()?;
        self.execute_stream_with_openai_options(
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

    pub async fn execute_stream_with_openai_options(
        &self,
        task: TaskRequest,
        model: Option<String>,
        family: Option<OpenAiCompatibleOptions>,
        provider: Option<OpenAiOptions>,
        execution: crate::ExecutionOptions,
    ) -> Result<crate::MessageResponseStream, crate::RuntimeError> {
        self.inner
            .execute_stream_on_attempt(
                task,
                self.openai_attempt(model, family, provider),
                execution,
            )
            .await
    }
}
