use agent_core::{Request, Response};

use crate::message_create_input::MessageCreateInput;
use crate::provider_client::ProviderClient;
use crate::runtime_error::RuntimeError;
use crate::types::ResponseMeta;

#[derive(Debug, Clone)]
pub struct DirectMessagesApi<'a> {
    pub client: &'a ProviderClient,
}

impl DirectMessagesApi<'_> {
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
    ) -> Result<Response, RuntimeError> {
        self.client.create(input.into()).await
    }

    pub async fn create_with_meta(
        &self,
        input: impl Into<MessageCreateInput>,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.client.create_with_meta(input.into()).await
    }

    pub async fn create_request(&self, request: Request) -> Result<Response, RuntimeError> {
        self.client.send(request).await
    }

    pub async fn create_request_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.client.send_with_meta(request).await
    }
}
