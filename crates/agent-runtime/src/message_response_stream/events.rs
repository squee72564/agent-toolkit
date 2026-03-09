use std::sync::Arc;
use std::time::Instant;

use crate::message_response_stream::state::{
    AttemptContext, CompletedAttemptContext, StreamDriverState,
};
use crate::observer::{RuntimeObserver, safe_call_observer};
use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::types::{
    AttemptFailureEvent, AttemptMeta, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent,
};

pub(super) fn attempt_failure_meta(context: &AttemptContext, error: &RuntimeError) -> AttemptMeta {
    AttemptMeta {
        provider: context.provider,
        model: context.model.clone(),
        success: false,
        status_code: error.status_code.or(context.status_code),
        request_id: error
            .request_id
            .clone()
            .or_else(|| context.request_id.clone()),
        error_kind: Some(error.kind),
        error_message: Some(error.message.clone()),
    }
}

pub(super) fn emit_attempt_start(
    observer: Option<&Arc<dyn RuntimeObserver>>,
    provider: agent_core::ProviderId,
    model: Option<String>,
    target_index: usize,
    attempt_index: usize,
    started_at: Instant,
) {
    let event = AttemptStartEvent {
        request_id: None,
        provider: Some(provider),
        model,
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed: started_at.elapsed(),
    };
    safe_call_observer(observer, |runtime_observer| {
        runtime_observer.on_attempt_start(&event);
    });
}

pub(super) fn emit_attempt_success(
    request_observer: &Option<Arc<dyn RuntimeObserver>>,
    attempt: &CompletedAttemptContext,
) {
    let event = AttemptSuccessEvent {
        request_id: attempt.meta.request_id.clone(),
        provider: Some(attempt.meta.provider),
        model: Some(attempt.meta.model.clone()),
        target_index: Some(attempt.target_index),
        attempt_index: Some(attempt.attempt_index),
        elapsed: attempt.started_at.elapsed(),
        status_code: attempt.meta.status_code,
    };
    safe_call_observer(
        attempt.observer.as_ref().or(request_observer.as_ref()),
        |observer| {
            observer.on_attempt_success(&event);
        },
    );
}

pub(super) fn emit_attempt_failure(
    observer: Option<&Arc<dyn RuntimeObserver>>,
    meta: &AttemptMeta,
    target_index: usize,
    attempt_index: usize,
    started_at: Instant,
) {
    let event = AttemptFailureEvent {
        request_id: meta.request_id.clone(),
        provider: Some(meta.provider),
        model: Some(meta.model.clone()),
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed: started_at.elapsed(),
        error_kind: meta.error_kind,
        error_message: meta.error_message.clone(),
    };
    safe_call_observer(observer, |runtime_observer| {
        runtime_observer.on_attempt_failure(&event);
    });
}

pub(super) fn emit_request_end_success(
    state: &StreamDriverState,
    attempt: &CompletedAttemptContext,
) {
    let event = RequestEndEvent {
        request_id: attempt.meta.request_id.clone(),
        provider: Some(attempt.meta.provider),
        model: Some(attempt.meta.model.clone()),
        target_index: Some(attempt.target_index),
        attempt_index: Some(attempt.attempt_index),
        elapsed: state.request_started_at.elapsed(),
        status_code: attempt.meta.status_code,
        error_kind: None,
        error_message: None,
    };
    safe_call_observer(state.request_observer.as_ref(), |observer| {
        observer.on_request_end(&event);
    });
}

pub(super) fn emit_request_end_failure(
    state: &StreamDriverState,
    attempt: &AttemptContext,
    error: &RuntimeError,
) {
    let terminal_error = terminal_failure_error(error);
    let event = RequestEndEvent {
        request_id: terminal_error
            .request_id
            .clone()
            .or_else(|| attempt.request_id.clone()),
        provider: terminal_error.provider.or(Some(attempt.provider)),
        model: Some(attempt.model.clone()),
        target_index: Some(attempt.target_index),
        attempt_index: Some(attempt.attempt_index),
        elapsed: state.request_started_at.elapsed(),
        status_code: terminal_error.status_code.or(attempt.status_code),
        error_kind: Some(terminal_error.kind),
        error_message: Some(terminal_error.message.clone()),
    };
    safe_call_observer(state.request_observer.as_ref(), |observer| {
        observer.on_request_end(&event);
    });
}

pub(crate) fn event_model(target_model: Option<&str>, request_model: &str) -> Option<String> {
    trimmed_non_empty(target_model.unwrap_or_default())
        .or_else(|| trimmed_non_empty(request_model))
        .map(ToString::to_string)
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn terminal_failure_error(error: &RuntimeError) -> &RuntimeError {
    if error.kind == RuntimeErrorKind::FallbackExhausted
        && let Some(source) = error.source_ref()
        && let Some(terminal_error) = source.downcast_ref::<RuntimeError>()
    {
        return terminal_error;
    }
    error
}
