use agent_core::{Request, Response};

use crate::agent_toolkit::AgentToolkit;
use crate::message_create_input::MessageCreateInput;
use crate::runtime_error::RuntimeError;
use crate::send_options::SendOptions;
use crate::types::ResponseMeta;

/// Non-streaming API for routed multi-provider execution.
#[derive(Debug, Clone)]
pub struct RoutedMessagesApi<'a> {
    toolkit: &'a AgentToolkit,
}

impl RoutedMessagesApi<'_> {
    pub(crate) fn new(toolkit: &AgentToolkit) -> RoutedMessagesApi<'_> {
        RoutedMessagesApi { toolkit }
    }

    /// Builds a request from [`MessageCreateInput`] and executes it using the
    /// supplied routing options.
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
        options: SendOptions,
    ) -> Result<Response, RuntimeError> {
        self.create_with_meta(input, options)
            .await
            .map(|(response, _)| response)
    }

    /// Like [`Self::create`], but also returns metadata for the selected target
    /// and all attempts.
    pub async fn create_with_meta(
        &self,
        input: impl Into<MessageCreateInput>,
        options: SendOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let request = input.into().into_request_with_options(None, true)?;
        self.toolkit.send_with_meta(request, options).await
    }

    /// Sends a fully-formed non-streaming request through the routed execution
    /// path.
    pub async fn create_request(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<Response, RuntimeError> {
        self.toolkit.send(request, options).await
    }

    /// Like [`Self::create_request`], but also returns metadata for the
    /// selected target and all attempts.
    pub async fn create_request_with_meta(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.toolkit.send_with_meta(request, options).await
    }
}
