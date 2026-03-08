use agent_core::{Request, Response};

use crate::agent_toolkit::AgentToolkit;
use crate::message_create_input::MessageCreateInput;
use crate::runtime_error::RuntimeError;
use crate::send_options::SendOptions;
use crate::types::ResponseMeta;

#[derive(Debug, Clone)]
pub struct RouterMessagesApi<'a> {
    pub toolkit: &'a AgentToolkit,
}

impl RouterMessagesApi<'_> {
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
        options: SendOptions,
    ) -> Result<Response, RuntimeError> {
        self.create_with_meta(input, options)
            .await
            .map(|(response, _)| response)
    }

    pub async fn create_with_meta(
        &self,
        input: impl Into<MessageCreateInput>,
        options: SendOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let request = input.into().into_request_with_options(None, true)?;
        self.toolkit.send_with_meta(request, options).await
    }

    pub async fn create_request(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<Response, RuntimeError> {
        self.toolkit.send(request, options).await
    }

    pub async fn create_request_with_meta(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.toolkit.send_with_meta(request, options).await
    }
}
