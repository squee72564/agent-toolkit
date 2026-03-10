use agent_core::Request;

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

    /// Sends a fully-formed request directly to this client.
    ///
    /// The request must have `stream = true`.
    pub async fn create_request(
        &self,
        request: Request,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.client.send_stream(request).await
    }
}
