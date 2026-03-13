use agent_core::{Response, TaskRequest};

use crate::agent_toolkit::AgentToolkit;
use crate::execution_options::ExecutionOptions;
use crate::message_create_input::MessageCreateInput;
use crate::route::Route;
use crate::runtime_error::RuntimeError;
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
    /// supplied route and execution options.
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.create_with_meta(input, route, execution)
            .await
            .map(|(response, _)| response)
    }

    /// Like [`Self::create`], but also returns metadata for the selected target
    /// and all attempts.
    pub async fn create_with_meta(
        &self,
        input: impl Into<MessageCreateInput>,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let input = input.into();
        let (task, model_override, input_execution) = input.into_task_request_parts()?;
        let route = apply_model_override(route, model_override);
        let execution = merge_execution_options(input_execution, execution);
        self.toolkit.execute_with_meta(task, route, execution).await
    }

    /// Executes an explicit semantic task over a routed attempt chain.
    pub async fn create_task(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.toolkit.execute(task, route, execution).await
    }

    /// Like [`Self::create_task`], but also returns metadata for the selected
    /// target and all attempts.
    pub async fn create_task_with_meta(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.toolkit.execute_with_meta(task, route, execution).await
    }
}

pub(crate) fn apply_model_override(route: Route, model_override: Option<String>) -> Route {
    let Some(model_override) = model_override else {
        return route;
    };

    let primary = route.primary.with_model(model_override);
    Route::to(primary)
        .with_fallbacks(route.fallbacks)
        .with_fallback_policy(route.fallback_policy)
}

pub(crate) fn merge_execution_options(
    input_execution: ExecutionOptions,
    mut execution: ExecutionOptions,
) -> ExecutionOptions {
    execution.response_mode = input_execution.response_mode;
    if execution.observer.is_none() {
        execution.observer = input_execution.observer;
    }
    if execution.transport == crate::TransportOptions::default() {
        execution.transport = input_execution.transport;
    }
    execution
}
