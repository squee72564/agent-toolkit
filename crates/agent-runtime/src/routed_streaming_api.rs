use agent_core::TaskRequest;

use crate::agent_toolkit::AgentToolkit;
use crate::execution_options::ExecutionOptions;
use crate::message_create_input::MessageCreateInput;
use crate::message_response_stream::MessageResponseStream;
use crate::route::Route;
use crate::runtime_error::RuntimeError;

/// Streaming API for routed multi-provider execution.
#[derive(Debug, Clone)]
pub struct RoutedStreamingApi<'a> {
    toolkit: &'a AgentToolkit,
}

impl RoutedStreamingApi<'_> {
    pub(crate) fn new(toolkit: &AgentToolkit) -> RoutedStreamingApi<'_> {
        RoutedStreamingApi { toolkit }
    }

    /// Builds a streaming request from [`MessageCreateInput`] and opens a
    /// routed stream using the supplied route and execution options.
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        let input = input.into();
        let (task, model_override, input_execution) = input.into_task_request_parts()?;
        let route = super::routed_messages_api::apply_model_override(route, model_override);
        let mut execution =
            super::routed_messages_api::merge_execution_options(input_execution, execution);
        execution.response_mode = crate::ResponseMode::Streaming;
        self.toolkit.execute_stream(task, route, execution).await
    }

    /// Opens a routed stream for an explicit semantic task.
    pub async fn create_task(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.toolkit.execute_stream(task, route, execution).await
    }
}
