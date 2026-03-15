use std::time::Instant;

use crate::message_response_stream::events::{
    emit_attempt_failure, emit_attempt_start, emit_request_end_failure,
};
use crate::message_response_stream::state::{
    AttemptContext, CompletedAttemptContext, LiveAttempt, PendingCompletion, RoutingState,
    StreamDriverState,
};
use crate::observer::attempt_skipped_event;
use crate::observer::{resolve_observer_for_request, safe_call_observer};
use crate::planner;
use crate::provider_runtime::ProviderStreamAttemptOutcome;
use crate::runtime_error::RuntimeError;
use crate::types::{failed_attempt_record, succeeded_attempt_record};

pub(super) enum DriveNextOutcome {
    Envelope(agent_core::CanonicalStreamEnvelope),
    TerminalError(RuntimeError),
    Exhausted,
}

pub(super) async fn drive_next(
    mut state: StreamDriverState,
) -> (StreamDriverState, DriveNextOutcome) {
    loop {
        if let Some(error) = state.terminal_error.clone() {
            return (state, DriveNextOutcome::TerminalError(error));
        }

        let Some(mut attempt) = state.current_attempt.take() else {
            state.terminal_error = Some(RuntimeError::configuration(
                "stream driver reached an invalid state without an active attempt",
            ));
            continue;
        };

        match attempt.stream.next_envelope().await {
            Ok(Some(envelope)) => {
                state.note_emitted_envelope(&envelope);
                state.current_attempt = Some(attempt);
                return (state, DriveNextOutcome::Envelope(envelope));
            }
            Ok(None) => match attempt.stream.finish() {
                Ok((response, http_response)) => {
                    let status_code = Some(http_response.head.status.as_u16());
                    let request_id = http_response.head.request_id.clone();
                    state.note_attempt_success(&attempt.context, status_code, request_id.clone());
                    let success_meta =
                        state.success_meta(&attempt.context, status_code, request_id.clone());
                    state.pending_completion = Some(PendingCompletion {
                        response,
                        attempt: CompletedAttemptContext {
                            target_index: attempt.context.target_index,
                            attempt_index: attempt.context.attempt_index,
                            started_at: attempt.context.started_at,
                            observer: attempt.context.observer.clone(),
                            record: succeeded_attempt_record(
                                attempt.context.provider_instance.clone(),
                                attempt.context.provider,
                                attempt.context.model.clone(),
                                attempt.context.target_index,
                                attempt.context.attempt_index,
                                status_code,
                                request_id,
                            ),
                        },
                        success_meta,
                    });
                    return (state, DriveNextOutcome::Exhausted);
                }
                Err(error) => {
                    if let Some(next_attempt) =
                        try_open_fallback_attempt(&mut state, &attempt.context, &error).await
                    {
                        let failure_record = failed_attempt_record(
                            attempt.context.provider_instance.clone(),
                            attempt.context.provider,
                            attempt.context.model.clone(),
                            attempt.context.target_index,
                            attempt.context.attempt_index,
                            &error,
                        );
                        emit_attempt_failure(
                            attempt.context.observer.as_ref(),
                            &failure_record,
                            attempt.context.started_at,
                        );
                        state.note_attempt_failure(&attempt.context, &error);
                        state.current_attempt = Some(next_attempt);
                        continue;
                    }
                    let failure_record = failed_attempt_record(
                        attempt.context.provider_instance.clone(),
                        attempt.context.provider,
                        attempt.context.model.clone(),
                        attempt.context.target_index,
                        attempt.context.attempt_index,
                        &error,
                    );
                    emit_attempt_failure(
                        attempt.context.observer.as_ref(),
                        &failure_record,
                        attempt.context.started_at,
                    );
                    state.note_attempt_failure(&attempt.context, &error);
                    emit_request_end_failure(&state, &attempt.context, &error);
                    let failure_meta = state.executed_failure_meta(&attempt.context, &error);
                    let error = error.with_executed_failure_meta(failure_meta);
                    state.terminal_error = Some(error.clone());
                    return (state, DriveNextOutcome::TerminalError(error));
                }
            },
            Err(error) => {
                if let Some(next_attempt) =
                    try_open_fallback_attempt(&mut state, &attempt.context, &error).await
                {
                    let failure_record = failed_attempt_record(
                        attempt.context.provider_instance.clone(),
                        attempt.context.provider,
                        attempt.context.model.clone(),
                        attempt.context.target_index,
                        attempt.context.attempt_index,
                        &error,
                    );
                    emit_attempt_failure(
                        attempt.context.observer.as_ref(),
                        &failure_record,
                        attempt.context.started_at,
                    );
                    state.note_attempt_failure(&attempt.context, &error);
                    state.current_attempt = Some(next_attempt);
                    continue;
                }
                let failure_record = failed_attempt_record(
                    attempt.context.provider_instance.clone(),
                    attempt.context.provider,
                    attempt.context.model.clone(),
                    attempt.context.target_index,
                    attempt.context.attempt_index,
                    &error,
                );
                emit_attempt_failure(
                    attempt.context.observer.as_ref(),
                    &failure_record,
                    attempt.context.started_at,
                );
                state.note_attempt_failure(&attempt.context, &error);
                emit_request_end_failure(&state, &attempt.context, &error);
                let failure_meta = state.executed_failure_meta(&attempt.context, &error);
                let error = error.with_executed_failure_meta(failure_meta);
                state.terminal_error = Some(error.clone());
                return (state, DriveNextOutcome::TerminalError(error));
            }
        }
    }
}

async fn try_open_fallback_attempt(
    state: &mut StreamDriverState,
    current_attempt: &AttemptContext,
    error: &RuntimeError,
) -> Option<LiveAttempt> {
    if !state.can_fallback() {
        return None;
    }

    let RoutingState::Routed(routed) = &state.routing else {
        return None;
    };

    if !routed.fallback_policy.should_retry_next_target(
        error,
        current_attempt.provider,
        &current_attempt.provider_instance,
    ) {
        return None;
    }

    loop {
        let (
            index,
            attempt,
            toolkit,
            task,
            execution,
            fallback_policy,
            planning_rejection_policy,
            total_attempts,
        ) = {
            let RoutingState::Routed(routed) = &mut state.routing else {
                return None;
            };

            if routed.next_target_index >= routed.attempts.len() {
                return None;
            }

            let index = routed.next_target_index;
            routed.next_target_index = routed.next_target_index.saturating_add(1);
            (
                index,
                routed.attempts[index].clone(),
                routed.toolkit.clone(),
                routed.task.clone(),
                routed.execution.clone(),
                routed.fallback_policy.clone(),
                routed.planning_rejection_policy,
                routed.attempts.len(),
            )
        };
        let target = attempt.target.clone();
        let client = toolkit.clients.get(&target.instance)?;
        let execution_plan = match planner::plan_fallback_attempt(
            client,
            &attempt,
            &task,
            &execution,
            planning_rejection_policy,
            index,
            total_attempts,
        ) {
            planner::FallbackPlanResult::Executable(plan) => *plan,
            planner::FallbackPlanResult::Skip(skipped) => {
                let provider_instance = skipped.record.provider_instance.clone();
                let event = attempt_skipped_event(&skipped.record, skipped.elapsed);
                state.note_attempt_skipped(skipped.record.clone());
                emit_attempt_skipped(&toolkit, &execution, &provider_instance, &event);
                continue;
            }
            planner::FallbackPlanResult::Rejected(skipped) => {
                let provider_instance = skipped.record.provider_instance.clone();
                let event = attempt_skipped_event(&skipped.record, skipped.elapsed);
                state.note_attempt_skipped(skipped.record.clone());
                emit_attempt_skipped(&toolkit, &execution, &provider_instance, &event);
                return None;
            }
            planner::FallbackPlanResult::Stop => return None,
        };
        let observer = resolve_observer_for_request(
            client.runtime.observer.as_ref(),
            toolkit.observer.as_ref(),
            execution.observer.as_ref(),
        )
        .cloned();
        let attempt_started_at = Instant::now();
        emit_attempt_start(
            observer.as_ref(),
            client.runtime.kind,
            Some(execution_plan.provider_attempt.model.clone()),
            index,
            index,
            attempt_started_at,
        );
        match client
            .runtime
            .open_stream_attempt(execution_plan.clone())
            .await
        {
            ProviderStreamAttemptOutcome::Opened {
                stream,
                selected_model,
                status_code,
                request_id,
            } => {
                return Some(LiveAttempt {
                    stream: *stream,
                    context: AttemptContext {
                        target_index: index,
                        attempt_index: index,
                        started_at: attempt_started_at,
                        observer,
                        provider_instance: target.instance.clone(),
                        provider: client.runtime.kind,
                        model: selected_model,
                        request_id,
                        status_code,
                    },
                });
            }
            ProviderStreamAttemptOutcome::Failure {
                error: attempt_error,
                selected_model,
            } => {
                let failed_attempt_record = failed_attempt_record(
                    target.instance.clone(),
                    client.runtime.kind,
                    selected_model.clone(),
                    index,
                    index,
                    &attempt_error,
                );
                emit_attempt_failure(
                    observer.as_ref(),
                    &failed_attempt_record,
                    attempt_started_at,
                );
                let provider = client.runtime.kind;
                let model = selected_model;
                let request_id = attempt_error.request_id.clone();
                let status_code = attempt_error.status_code;
                let provider_instance = target.instance.clone();
                let observer_for_end = observer.clone();
                state.note_failed_open_attempt(
                    provider_instance.clone(),
                    provider,
                    model.clone(),
                    index,
                    index,
                    &attempt_error,
                );
                let has_more_targets = matches!(
                    &state.routing,
                    RoutingState::Routed(routed) if routed.next_target_index < routed.attempts.len()
                );
                let should_continue = has_more_targets
                    && fallback_policy.should_retry_next_target(
                        &attempt_error,
                        client.runtime.kind,
                        &target.instance,
                    );
                if !should_continue {
                    let failed_attempt = AttemptContext {
                        target_index: index,
                        attempt_index: index,
                        started_at: attempt_started_at,
                        observer: observer_for_end.clone(),
                        provider_instance: provider_instance.clone(),
                        provider,
                        model: model.clone(),
                        request_id,
                        status_code,
                    };
                    let failure_meta = state.executed_failure_meta(&failed_attempt, &attempt_error);
                    let attempt_error = attempt_error.with_executed_failure_meta(failure_meta);
                    emit_request_end_failure(
                        state,
                        &AttemptContext {
                            observer: observer_for_end,
                            request_id: attempt_error.request_id.clone(),
                            ..failed_attempt
                        },
                        &attempt_error,
                    );
                    state.terminal_error = Some(attempt_error);
                    return None;
                }
            }
        }
    }
}

fn emit_attempt_skipped(
    toolkit: &crate::AgentToolkit,
    execution: &crate::ExecutionOptions,
    provider_instance: &agent_core::ProviderInstanceId,
    event: &crate::AttemptSkippedEvent,
) {
    let observer = toolkit.resolve_attempt_observer(execution, provider_instance.clone());
    safe_call_observer(observer.as_ref(), |runtime_observer| {
        runtime_observer.on_attempt_skipped(event);
    });
}
