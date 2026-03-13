use agent_core::{Response, TaskRequest};

use crate::execution_options::ExecutionOptions;
use crate::message_create_input::MessageCreateInput;
use crate::provider_client::ProviderClient;
use crate::runtime_error::RuntimeError;
use crate::types::ResponseMeta;

/// Non-streaming API for a single provider client.
#[derive(Debug, Clone)]
pub struct DirectMessagesApi<'a> {
    client: &'a ProviderClient,
}

impl DirectMessagesApi<'_> {
    pub(crate) fn new(client: &ProviderClient) -> DirectMessagesApi<'_> {
        DirectMessagesApi { client }
    }

    /// Builds a request from [`MessageCreateInput`] and executes it against the
    /// provider associated with this client.
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
    ) -> Result<Response, RuntimeError> {
        self.client.create(input.into()).await
    }

    /// Like [`Self::create`], but also returns attempt metadata.
    pub async fn create_with_meta(
        &self,
        input: impl Into<MessageCreateInput>,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.client.create_with_meta(input.into()).await
    }

    /// Executes an explicit semantic task using the client's configured
    /// default model unless `model_override` is supplied.
    pub async fn create_task(
        &self,
        task: TaskRequest,
        model_override: Option<String>,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.client.execute(task, model_override, execution).await
    }

    /// Like [`Self::create_task`], but also returns attempt metadata.
    pub async fn create_task_with_meta(
        &self,
        task: TaskRequest,
        model_override: Option<String>,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.client
            .execute_with_meta(task, model_override, execution)
            .await
    }
}
