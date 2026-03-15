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
    /// supplied route with default execution options.
    pub async fn create(
        &self,
        input: impl Into<MessageCreateInput>,
        route: Route,
    ) -> Result<Response, RuntimeError> {
        self.create_with_meta(input, route)
            .await
            .map(|(response, _)| response)
    }

    /// Like [`Self::create`], but also returns metadata for the selected target
    /// and all attempts.
    pub async fn create_with_meta(
        &self,
        input: impl Into<MessageCreateInput>,
        route: Route,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.create_with_meta_and_options(input, route, ExecutionOptions::default())
            .await
    }

    /// Builds a request from [`MessageCreateInput`] and executes it using the
    /// supplied route and explicit execution options.
    pub async fn create_with_options(
        &self,
        input: impl Into<MessageCreateInput>,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.create_with_meta_and_options(input, route, execution)
            .await
            .map(|(response, _)| response)
    }

    /// Like [`Self::create_with_options`], but also returns metadata for the
    /// selected target and all attempts.
    pub async fn create_with_meta_and_options(
        &self,
        input: impl Into<MessageCreateInput>,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.toolkit
            .execute_with_meta(input.into().into_task_request()?, route, execution)
            .await
    }

    /// Executes an explicit semantic task over a routed attempt chain.
    pub async fn execute(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.toolkit.execute(task, route, execution).await
    }

    /// Like [`Self::execute`], but also returns metadata for the selected
    /// target and all attempts.
    pub async fn execute_with_meta(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.toolkit.execute_with_meta(task, route, execution).await
    }

    /// Executes an explicit semantic task over a routed attempt chain.
    pub async fn create_task(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.execute(task, route, execution).await
    }

    /// Like [`Self::create_task`], but also returns metadata for the selected
    /// target and all attempts.
    pub async fn create_task_with_meta(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.execute_with_meta(task, route, execution).await
    }
}
