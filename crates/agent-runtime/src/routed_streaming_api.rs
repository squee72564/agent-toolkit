use agent_core::Request;

use crate::agent_toolkit::AgentToolkit;
use crate::message_create_input::MessageCreateInput;
use crate::message_response_stream::MessageResponseStream;
use crate::runtime_error::RuntimeError;
use crate::send_options::SendOptions;

#[derive(Debug, Clone)]
pub struct RoutedStreamingApi<'a> {
    toolkit: &'a AgentToolkit,
}

impl RoutedStreamingApi<'_> {
    pub(crate) fn new(toolkit: &AgentToolkit) -> RoutedStreamingApi<'_> {
        RoutedStreamingApi { toolkit }
    }

    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
        options: SendOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.toolkit.create_stream(input.into(), options).await
    }

    pub async fn create_request(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.toolkit.send_stream(request, options).await
    }
}
