use agent_core::TaskRequest;

use crate::execution_options::ExecutionOptions;
use crate::message_create_input::MessageCreateInput;
use crate::message_response_stream::MessageResponseStream;
use crate::provider_client::ProviderClient;
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
    /// configured default model unless `model_override` is supplied.
    pub async fn create_task(
        &self,
        task: TaskRequest,
        model_override: Option<String>,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.client
            .execute_stream(task, model_override, execution)
            .await
    }
}
