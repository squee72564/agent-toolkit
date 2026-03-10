use agent_core::{Request, Response};

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

    /// Sends a fully-formed non-streaming request directly to this client.
    pub async fn create_request(&self, request: Request) -> Result<Response, RuntimeError> {
        self.client.send(request).await
    }

    /// Like [`Self::create_request`], but also returns attempt metadata.
    pub async fn create_request_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.client.send_with_meta(request).await
    }
}
