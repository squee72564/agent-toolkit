use std::{collections::HashMap, sync::Arc};

use agent_core::{ProviderInstanceId, Response, TaskRequest};

use crate::attempt_spec::AttemptSpec;
use crate::execution_options::{ExecutionOptions, ResponseMode};
use crate::message_response_stream::{
    AttemptContext, LiveAttempt, MessageResponseStream, RoutedStreamInit,
};
use crate::observer::RuntimeObserver;
use crate::planner;
use crate::provider_client::ProviderClient;
use crate::provider_runtime::{ProviderAttemptOutcome, ProviderStreamAttemptOutcome};
use crate::route::Route;
use crate::routed_messages_api::RoutedMessagesApi;
use crate::routed_streaming_api::RoutedStreamingApi;
use crate::runtime_error::RuntimeError;
use crate::types::ResponseMeta;

mod builder;
mod execution;

pub use self::builder::AgentToolkitBuilder;
use self::execution::PreparedExecution;

/// Multi-provider runtime for routed request execution.
///
/// `AgentToolkit` is the high-level entry point when a request may need an
/// explicit [`crate::Target`], a [`crate::FallbackPolicy`], or request-scoped
/// observability through [`RuntimeObserver`].
#[derive(Clone)]
pub struct AgentToolkit {
    pub(crate) clients: HashMap<ProviderInstanceId, ProviderClient>,
    pub(crate) observer: Option<Arc<dyn RuntimeObserver>>,
}

impl std::fmt::Debug for AgentToolkit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentToolkit")
            .field("clients", &self.clients)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
}

impl AgentToolkit {
    /// Creates a builder for configuring the providers available to the
    /// toolkit.
    pub fn builder() -> AgentToolkitBuilder {
        AgentToolkitBuilder::default()
    }

    /// Returns the non-streaming routed request API.
    pub fn messages(&self) -> RoutedMessagesApi<'_> {
        RoutedMessagesApi::new(self)
    }

    /// Returns the streaming routed request API.
    pub fn streaming(&self) -> RoutedStreamingApi<'_> {
        RoutedStreamingApi::new(self)
    }

    /// Executes a semantic task over the supplied route.
    pub async fn execute(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.execute_with_meta(task, route, execution)
            .await
            .map(|(response, _)| response)
    }

    /// Like [`Self::execute`], but also returns metadata for the selected
    /// target and all attempts.
    pub async fn execute_with_meta(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        if execution.response_mode != ResponseMode::NonStreaming {
            return Err(RuntimeError::configuration(
                "messages() requires ExecutionOptions.response_mode = ResponseMode::NonStreaming",
            ));
        }

        let prepared = PreparedExecution::new(self, &route, &execution)?;
        prepared.emit_request_start(None);

        let fallback_policy = route.fallback_policy.clone();
        let mut attempts = Vec::new();
        let mut last_error: Option<RuntimeError> = None;
        let initial_plan = match planner::plan_routed_execution(
            self,
            &prepared.attempts,
            &task,
            &execution,
            route.planning_rejection_policy,
        ) {
            planner::RoutedPlanningResult::Executable(plan) => plan,
            planner::RoutedPlanningResult::PlanningFailure(failure) => {
                let error = RuntimeError::route_planning_failure(failure);
                prepared.emit_request_end_failure(None, None, None, None, &error);
                return Err(error);
            }
            planner::RoutedPlanningResult::Fatal(error) => {
                prepared.emit_request_end_failure(None, None, None, None, &error);
                return Err(error);
            }
        };

        for (index, attempt_spec) in prepared
            .attempts
            .iter()
            .enumerate()
            .skip(initial_plan.target_index)
        {
            let target = &attempt_spec.target;
            let Some(client) = self.clients.get(&target.instance) else {
                let error = RuntimeError::target_resolution(format!(
                    "provider instance {} is not registered",
                    target.instance
                ));
                prepared.emit_request_end_failure(None, None, None, None, &error);
                return Err(error);
            };
            let execution_plan = if index == initial_plan.target_index {
                initial_plan.execution_plan.clone()
            } else {
                match planner::plan_fallback_attempt(
                    client,
                    attempt_spec,
                    &task,
                    &execution,
                    route.planning_rejection_policy,
                    index,
                    prepared.attempts.len(),
                ) {
                    planner::FallbackPlanResult::Executable(plan) => plan,
                    planner::FallbackPlanResult::Skip => continue,
                    planner::FallbackPlanResult::Stop => break,
                }
            };
            let attempt_execution = prepared.attempt(
                self,
                &execution,
                Some(execution_plan.attempt.model.as_str()),
                target,
                index,
            );

            let attempt = client.runtime.execute_attempt(execution_plan).await;

            match attempt {
                ProviderAttemptOutcome::Success { response, meta } => {
                    attempt_execution.emit_success(&meta, index);

                    attempts.push(meta.clone());
                    let response_meta = attempt_execution.response_meta(attempts, meta);
                    let response_model = response_meta.selected_model.clone();
                    prepared.emit_request_end_success(
                        Some(response_meta.selected_provider),
                        Some(response_model),
                        Some(index),
                        Some(index),
                        response_meta.request_id.clone(),
                        response_meta.status_code,
                    );
                    return Ok((response, response_meta));
                }
                ProviderAttemptOutcome::Failure { error, meta } => {
                    attempt_execution.emit_failure(&meta, index);

                    attempts.push(meta);
                    let should_continue = index + 1 < prepared.attempts.len()
                        && fallback_policy.should_fallback(&error);
                    last_error = Some(error);
                    if !should_continue {
                        break;
                    }
                }
            }
        }

        let result = match last_error {
            Some(error) if attempts.len() > 1 => Err(RuntimeError::fallback_exhausted(error)),
            Some(error) => Err(error),
            None => Err(RuntimeError::target_resolution(
                "no target providers were resolved for this request",
            )),
        };

        if let Err(error) = &result {
            prepared.emit_terminal_request_end(self, &execution, &attempts, error);
        }

        result
    }

    /// Executes a semantic task over the supplied route and opens a routed stream.
    pub async fn execute_stream(
        &self,
        task: TaskRequest,
        route: Route,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        if execution.response_mode != ResponseMode::Streaming {
            return Err(RuntimeError::configuration(
                "streaming() requires ExecutionOptions.response_mode = ResponseMode::Streaming",
            ));
        }

        let prepared = PreparedExecution::new(self, &route, &execution)?;
        prepared.emit_request_start(None);
        let fallback_policy = route.fallback_policy.clone();

        let initial_plan = match planner::plan_routed_execution(
            self,
            &prepared.attempts,
            &task,
            &execution,
            route.planning_rejection_policy,
        ) {
            planner::RoutedPlanningResult::Executable(plan) => plan,
            planner::RoutedPlanningResult::PlanningFailure(failure) => {
                return Err(RuntimeError::route_planning_failure(failure));
            }
            planner::RoutedPlanningResult::Fatal(error) => return Err(error),
        };
        let planner::PlannedRoutedAttempt {
            execution_plan,
            target_index: index,
        } = *initial_plan;
        let attempt_spec = &prepared.attempts[index];
        let target = &attempt_spec.target;
        let Some(client) = self.clients.get(&target.instance) else {
            return Err(RuntimeError::target_resolution(format!(
                "provider instance {} is not registered",
                target.instance
            )));
        };
        let attempt_execution = prepared.attempt(
            self,
            &execution,
            Some(execution_plan.attempt.model.as_str()),
            target,
            index,
        );

        match client
            .runtime
            .open_stream_attempt(execution_plan.clone())
            .await
        {
            ProviderStreamAttemptOutcome::Opened { stream, meta } => {
                return Ok(MessageResponseStream::new_routed(RoutedStreamInit {
                    task: task.clone(),
                    toolkit: self,
                    execution: execution.clone(),
                    fallback_policy,
                    planning_rejection_policy: route.planning_rejection_policy,
                    request_started_at: prepared.request_started_at,
                    request_observer: prepared.request_observer.clone(),
                    attempts: prepared.attempts.clone(),
                    current_attempt: LiveAttempt {
                        stream: *stream,
                        context: AttemptContext {
                            target_index: index,
                            attempt_index: index,
                            started_at: attempt_execution.started_at,
                            observer: attempt_execution.observer,
                            provider: meta.provider,
                            model: meta.model,
                            request_id: meta.request_id,
                            status_code: meta.status_code,
                        },
                    },
                    next_target_index: index + 1,
                }));
            }
            ProviderStreamAttemptOutcome::Failure { error, meta } => {
                attempt_execution.emit_failure(&meta, index);
                let should_continue =
                    index + 1 < prepared.attempts.len() && fallback_policy.should_fallback(&error);
                if !should_continue {
                    prepared.emit_request_end_failure(
                        Some(meta.provider),
                        Some(meta.model),
                        Some(index),
                        Some(index),
                        &error,
                    );
                    return Err(error);
                }
            }
        }

        Err(RuntimeError::target_resolution(
            "no target providers were resolved for this request",
        ))
    }

    fn resolve_attempt_observer(
        &self,
        execution: &ExecutionOptions,
        provider: ProviderInstanceId,
    ) -> Option<Arc<dyn RuntimeObserver>> {
        self.clients.get(&provider).and_then(|client| {
            crate::observer::resolve_observer_for_request(
                client.runtime.observer.as_ref(),
                self.observer.as_ref(),
                execution.observer.as_ref(),
            )
            .cloned()
        })
    }

    pub fn resolve_route_targets(&self, route: &Route) -> Result<Vec<AttemptSpec>, RuntimeError> {
        planner::resolve_route_targets(self, route)
    }
}
