use agent_core::TaskRequest;

use crate::execution_options::ExecutionOptions;
use crate::message::MessageCreateInput;
use crate::message_response_stream::MessageResponseStream;
use crate::provider::ProviderClient;
use crate::routing::AttemptSpec;
use crate::runtime_error::RuntimeError;

/// Streaming API for a single provider client.
#[derive(Debug, Clone)]
pub struct DirectStreamingApi<'a> {
    client: &'a ProviderClient,
}

impl DirectStreamingApi<'_> {
    pub(crate) fn new(client: &ProviderClient) -> DirectStreamingApi<'_> {
        DirectStreamingApi { client }
    }

    /// Builds a streaming request from [`MessageCreateInput`] and opens the
    /// provider stream.
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.client.create_stream(input.into()).await
    }

    /// Opens a stream for an explicit semantic task using the client's
    /// configured default attempt target.
    pub async fn execute(
        &self,
        task: TaskRequest,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.client.execute_stream(task, execution).await
    }

    /// Opens a stream for an explicit semantic task on an explicit single
    /// attempt target scoped to this client.
    pub async fn execute_on_attempt(
        &self,
        task: TaskRequest,
        attempt: AttemptSpec,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.client
            .execute_stream_on_attempt(task, attempt, execution)
            .await
    }

    /// Opens a stream for an explicit semantic task using the client's
    /// configured default attempt target.
    pub async fn create_task(
        &self,
        task: TaskRequest,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.execute(task, execution).await
    }

    /// Opens a stream for an explicit semantic task on an explicit single
    /// attempt target scoped to this client.
    pub async fn create_task_on_attempt(
        &self,
        task: TaskRequest,
        attempt: AttemptSpec,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.execute_on_attempt(task, attempt, execution).await
    }
}
