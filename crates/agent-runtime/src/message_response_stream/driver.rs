use std::time::Instant;

use crate::message_response_stream::events::{
    attempt_failure_meta, emit_attempt_failure, emit_attempt_start, emit_request_end_failure,
    event_model,
};
use crate::message_response_stream::state::{
    AttemptContext, CompletedAttemptContext, LiveAttempt, PendingCompletion, RoutingState,
    StreamDriverState,
};
use crate::observer::resolve_observer_for_request;
use crate::provider_runtime::ProviderStreamAttemptOutcome;
use crate::runtime_error::RuntimeError;
use crate::types::AttemptMeta;

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
                state.first_envelope_emitted = true;
                state.current_attempt = Some(attempt);
                return (state, DriveNextOutcome::Envelope(envelope));
            }
            Ok(None) => match attempt.stream.finish() {
                Ok((response, http_response)) => {
                    let attempt_meta = AttemptMeta {
                        provider: attempt.context.provider,
                        model: attempt.context.model.clone(),
                        success: true,
                        status_code: Some(http_response.head.status.as_u16()),
                        request_id: http_response.head.request_id.clone(),
                        error_kind: None,
                        error_message: None,
                    };
                    state.pending_completion = Some(PendingCompletion {
                        response,
                        attempt: CompletedAttemptContext {
                            target_index: attempt.context.target_index,
                            attempt_index: attempt.context.attempt_index,
                            started_at: attempt.context.started_at,
                            observer: attempt.context.observer.clone(),
                            meta: attempt_meta,
                        },
                        selected_provider: attempt.context.provider,
                        selected_model: attempt.context.model.clone(),
                        status_code: Some(http_response.head.status.as_u16()),
                        request_id: http_response.head.request_id.clone(),
                    });
                    return (state, DriveNextOutcome::Exhausted);
                }
                Err(error) => {
                    if let Some(next_attempt) = try_open_fallback_attempt(&mut state, &error).await
                    {
                        let failure_meta = attempt_failure_meta(&attempt.context, &error);
                        emit_attempt_failure(
                            attempt.context.observer.as_ref(),
                            &failure_meta,
                            attempt.context.target_index,
                            attempt.context.attempt_index,
                            attempt.context.started_at,
                        );
                        state.attempts.push(failure_meta);
                        state.current_attempt = Some(next_attempt);
                        continue;
                    }
                    let failure_meta = attempt_failure_meta(&attempt.context, &error);
                    emit_attempt_failure(
                        attempt.context.observer.as_ref(),
                        &failure_meta,
                        attempt.context.target_index,
                        attempt.context.attempt_index,
                        attempt.context.started_at,
                    );
                    state.attempts.push(failure_meta);
                    emit_request_end_failure(&state, &attempt.context, &error);
                    state.terminal_error = Some(error.clone());
                    return (state, DriveNextOutcome::TerminalError(error));
                }
            },
            Err(error) => {
                if let Some(next_attempt) = try_open_fallback_attempt(&mut state, &error).await {
                    let failure_meta = attempt_failure_meta(&attempt.context, &error);
                    emit_attempt_failure(
                        attempt.context.observer.as_ref(),
                        &failure_meta,
                        attempt.context.target_index,
                        attempt.context.attempt_index,
                        attempt.context.started_at,
                    );
                    state.attempts.push(failure_meta);
                    state.current_attempt = Some(next_attempt);
                    continue;
                }
                let failure_meta = attempt_failure_meta(&attempt.context, &error);
                emit_attempt_failure(
                    attempt.context.observer.as_ref(),
                    &failure_meta,
                    attempt.context.target_index,
                    attempt.context.attempt_index,
                    attempt.context.started_at,
                );
                state.attempts.push(failure_meta);
                emit_request_end_failure(&state, &attempt.context, &error);
                state.terminal_error = Some(error.clone());
                return (state, DriveNextOutcome::TerminalError(error));
            }
        }
    }
}

async fn try_open_fallback_attempt(
    state: &mut StreamDriverState,
    error: &RuntimeError,
) -> Option<LiveAttempt> {
    if state.first_envelope_emitted {
        return None;
    }

    let RoutingState::Routed(routed) = &mut state.routing else {
        return None;
    };

    while routed.next_target_index < routed.targets.len() {
        if !routed
            .options
            .fallback_policy
            .as_ref()
            .is_some_and(|policy| policy.should_fallback(error))
        {
            return None;
        }

        let index = routed.next_target_index;
        routed.next_target_index = routed.next_target_index.saturating_add(1);
        let target = routed.targets[index].clone();
        let client = routed.toolkit.clients.get(&target.provider)?;
        let observer = resolve_observer_for_request(
            client.runtime.observer.as_ref(),
            routed.toolkit.observer.as_ref(),
            routed.options.observer.as_ref(),
        )
        .cloned();
        let attempt_started_at = Instant::now();
        emit_attempt_start(
            observer.as_ref(),
            target.provider,
            event_model(target.model.as_deref(), &state.request_model_id),
            index,
            index,
            attempt_started_at,
        );
        let request = routed.request.clone();
        match client
            .runtime
            .open_stream_attempt(
                request,
                target.model.as_deref(),
                routed.options.metadata.clone(),
            )
            .await
        {
            ProviderStreamAttemptOutcome::Opened { stream, meta } => {
                return Some(LiveAttempt {
                    stream: *stream,
                    context: AttemptContext {
                        target_index: index,
                        attempt_index: index,
                        started_at: attempt_started_at,
                        observer,
                        provider: meta.provider,
                        model: meta.model,
                        request_id: meta.request_id,
                        status_code: meta.status_code,
                    },
                });
            }
            ProviderStreamAttemptOutcome::Failure {
                error: attempt_error,
                meta,
            } => {
                emit_attempt_failure(observer.as_ref(), &meta, index, index, attempt_started_at);
                state.attempts.push(meta);
                if routed.next_target_index >= routed.targets.len()
                    || !routed
                        .options
                        .fallback_policy
                        .as_ref()
                        .is_some_and(|policy| policy.should_fallback(&attempt_error))
                {
                    emit_request_end_failure(
                        state,
                        &AttemptContext {
                            target_index: index,
                            attempt_index: index,
                            started_at: attempt_started_at,
                            observer,
                            provider: target.provider,
                            model: target.model.unwrap_or_default(),
                            request_id: attempt_error.request_id.clone(),
                            status_code: attempt_error.status_code,
                        },
                        &attempt_error,
                    );
                    state.terminal_error = Some(attempt_error);
                    return None;
                }
            }
        }
    }

    None
}
