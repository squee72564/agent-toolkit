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
use crate::types::{
    AttemptRecord, ResponseMeta, executed_failure_meta, failed_attempt_record, response_meta,
    succeeded_attempt_record,
};

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
        let mut attempt_history: Vec<AttemptRecord> = Vec::new();
        let mut last_error: Option<RuntimeError> = None;
        let initial_plan = match planner::plan_routed_execution(
            self,
            &prepared.attempts,
            &task,
            &execution,
            route.planning_rejection_policy,
        ) {
            planner::RoutedPlanningResult::Executable(plan) => plan,
            planner::RoutedPlanningResult::PlanningFailure {
                failure,
                skipped_attempts,
            } => {
                for skipped in &skipped_attempts {
                    prepared.emit_attempt_skipped(self, &execution, skipped);
                }
                let error = RuntimeError::route_planning_failure(failure);
                prepared.emit_request_end_failure(None, None, None, None, &error);
                return Err(error);
            }
            planner::RoutedPlanningResult::Fatal(error) => {
                prepared.emit_request_end_failure(None, None, None, None, &error);
                return Err(error);
            }
        };
        for skipped in &initial_plan.skipped_attempts {
            prepared.emit_attempt_skipped(self, &execution, skipped);
            attempt_history.push(skipped.record.clone());
        }

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
                    planner::FallbackPlanResult::Executable(plan) => *plan,
                    planner::FallbackPlanResult::Skip(skipped) => {
                        prepared.emit_attempt_skipped(self, &execution, &skipped);
                        attempt_history.push(skipped.record);
                        continue;
                    }
                    planner::FallbackPlanResult::Rejected(skipped) => {
                        prepared.emit_attempt_skipped(self, &execution, &skipped);
                        attempt_history.push(skipped.record);
                        break;
                    }
                    planner::FallbackPlanResult::Stop => break,
                }
            };
            let attempt_execution = prepared.attempt(
                self,
                &execution,
                Some(execution_plan.provider_attempt.model.as_str()),
                target,
                index,
            );

            let attempt = client.runtime.execute_attempt(execution_plan).await;

            match attempt {
                ProviderAttemptOutcome::Success {
                    response,
                    selected_model,
                    status_code,
                    request_id,
                } => {
                    let attempt_record = succeeded_attempt_record(
                        target.instance.clone(),
                        client.runtime.kind,
                        selected_model.clone(),
                        index,
                        index,
                        status_code,
                        request_id.clone(),
                    );
                    attempt_execution.emit_success(&attempt_record);
                    attempt_history.push(attempt_record);
                    let response_meta = response_meta(
                        target.instance.clone(),
                        client.runtime.kind,
                        selected_model,
                        status_code,
                        request_id,
                        attempt_history,
                    );
                    let response_model = response_meta.selected_model.clone();
                    prepared.emit_request_end_success(
                        Some(response_meta.selected_provider_kind),
                        Some(response_model),
                        Some(index),
                        Some(index),
                        response_meta.request_id.clone(),
                        response_meta.status_code,
                    );
                    return Ok((response, response_meta));
                }
                ProviderAttemptOutcome::Failure {
                    error,
                    selected_model,
                } => {
                    let attempt_record = failed_attempt_record(
                        target.instance.clone(),
                        client.runtime.kind,
                        selected_model.clone(),
                        index,
                        index,
                        &error,
                    );
                    attempt_execution.emit_failure(&attempt_record);
                    attempt_history.push(attempt_record);
                    let status_code = error.status_code;
                    let request_id = error.request_id.clone();
                    let error = error.with_executed_failure_meta(executed_failure_meta(
                        target.instance.clone(),
                        client.runtime.kind,
                        selected_model,
                        status_code,
                        request_id,
                        attempt_history.clone(),
                    ));
                    let should_continue = index + 1 < prepared.attempts.len()
                        && fallback_policy.should_retry_next_target(
                            &error,
                            client.runtime.kind,
                            &target.instance,
                        );
                    last_error = Some(error);
                    if !should_continue {
                        break;
                    }
                }
            }
        }

        let result = match last_error {
            Some(error)
                if attempt_history
                    .iter()
                    .filter(|attempt| {
                        !matches!(
                            attempt.disposition,
                            crate::AttemptDisposition::Skipped { .. }
                        )
                    })
                    .count()
                    > 1 =>
            {
                Err(RuntimeError::fallback_exhausted(error))
            }
            Some(error) => Err(error),
            None => Err(RuntimeError::target_resolution(
                "no target providers were resolved for this request",
            )),
        };

        if let Err(error) = &result {
            prepared.emit_terminal_request_end(self, &execution, &attempt_history, error);
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
        let mut attempt_history: Vec<AttemptRecord> = Vec::new();
        let mut last_error: Option<RuntimeError> = None;

        let initial_plan = match planner::plan_routed_execution(
            self,
            &prepared.attempts,
            &task,
            &execution,
            route.planning_rejection_policy,
        ) {
            planner::RoutedPlanningResult::Executable(plan) => plan,
            planner::RoutedPlanningResult::PlanningFailure {
                failure,
                skipped_attempts,
            } => {
                for skipped in &skipped_attempts {
                    prepared.emit_attempt_skipped(self, &execution, skipped);
                }
                let error = RuntimeError::route_planning_failure(failure);
                prepared.emit_request_end_failure(None, None, None, None, &error);
                return Err(error);
            }
            planner::RoutedPlanningResult::Fatal(error) => {
                prepared.emit_request_end_failure(None, None, None, None, &error);
                return Err(error);
            }
        };
        for skipped in &initial_plan.skipped_attempts {
            prepared.emit_attempt_skipped(self, &execution, skipped);
            attempt_history.push(skipped.record.clone());
        }
        let initial_plan = *initial_plan;

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
                    planner::FallbackPlanResult::Executable(plan) => *plan,
                    planner::FallbackPlanResult::Skip(skipped) => {
                        prepared.emit_attempt_skipped(self, &execution, &skipped);
                        attempt_history.push(skipped.record);
                        continue;
                    }
                    planner::FallbackPlanResult::Rejected(skipped) => {
                        prepared.emit_attempt_skipped(self, &execution, &skipped);
                        attempt_history.push(skipped.record);
                        break;
                    }
                    planner::FallbackPlanResult::Stop => break,
                }
            };
            let attempt_execution = prepared.attempt(
                self,
                &execution,
                Some(execution_plan.provider_attempt.model.as_str()),
                target,
                index,
            );

            match client.runtime.open_stream_attempt(execution_plan).await {
                ProviderStreamAttemptOutcome::Opened {
                    stream,
                    selected_model,
                    status_code,
                    request_id,
                } => {
                    return Ok(MessageResponseStream::new_routed(RoutedStreamInit {
                        task: task.clone(),
                        toolkit: self,
                        execution: execution.clone(),
                        fallback_policy,
                        planning_rejection_policy: route.planning_rejection_policy,
                        request_started_at: prepared.request_started_at,
                        request_observer: prepared.request_observer.clone(),
                        attempts: prepared.attempts.clone(),
                        attempt_history,
                        current_attempt: LiveAttempt {
                            stream: *stream,
                            context: AttemptContext {
                                target_index: index,
                                attempt_index: index,
                                started_at: attempt_execution.started_at,
                                observer: attempt_execution.observer,
                                provider_instance: target.instance.clone(),
                                provider: client.runtime.kind,
                                model: selected_model,
                                request_id,
                                status_code,
                            },
                        },
                        next_target_index: index + 1,
                    }));
                }
                ProviderStreamAttemptOutcome::Failure {
                    error,
                    selected_model,
                } => {
                    let attempt_record = failed_attempt_record(
                        target.instance.clone(),
                        client.runtime.kind,
                        selected_model.clone(),
                        index,
                        index,
                        &error,
                    );
                    attempt_execution.emit_failure(&attempt_record);
                    attempt_history.push(attempt_record);
                    let status_code = error.status_code;
                    let request_id = error.request_id.clone();
                    let error = error.with_executed_failure_meta(executed_failure_meta(
                        target.instance.clone(),
                        client.runtime.kind,
                        selected_model,
                        status_code,
                        request_id,
                        attempt_history.clone(),
                    ));
                    let should_continue = index + 1 < prepared.attempts.len()
                        && fallback_policy.should_retry_next_target(
                            &error,
                            client.runtime.kind,
                            &target.instance,
                        );
                    last_error = Some(error);
                    if !should_continue {
                        break;
                    }
                }
            }
        }

        let result = match last_error {
            Some(error)
                if attempt_history
                    .iter()
                    .filter(|attempt| {
                        !matches!(
                            attempt.disposition,
                            crate::AttemptDisposition::Skipped { .. }
                        )
                    })
                    .count()
                    > 1 =>
            {
                Err(RuntimeError::fallback_exhausted(error))
            }
            Some(error) => Err(error),
            None => Err(RuntimeError::target_resolution(
                "no target providers were resolved for this request",
            )),
        };

        if let Err(error) = &result {
            prepared.emit_terminal_request_end(self, &execution, &attempt_history, error);
        }

        result
    }

    pub(crate) fn resolve_attempt_observer(
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
