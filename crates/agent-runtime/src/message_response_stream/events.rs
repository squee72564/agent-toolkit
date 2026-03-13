use std::sync::Arc;
use std::time::Instant;

use crate::message_response_stream::state::{
    AttemptContext, CompletedAttemptContext, StreamDriverState,
};
use crate::observer::{RuntimeObserver, safe_call_observer};
use crate::runtime_error::RuntimeError;
use crate::types::{
    AttemptDisposition, AttemptMeta, RequestEndContext, attempt_failure_event, attempt_start_event,
    attempt_success_event, request_end_failure_event, request_end_success_event,
    terminal_failure_error,
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
    let event = attempt_start_event(
        provider,
        model,
        target_index,
        attempt_index,
        started_at.elapsed(),
    );
    safe_call_observer(observer, |runtime_observer| {
        runtime_observer.on_attempt_start(&event);
    });
}

pub(super) fn emit_attempt_success(
    request_observer: &Option<Arc<dyn RuntimeObserver>>,
    attempt: &CompletedAttemptContext,
) {
    let AttemptDisposition::Succeeded {
        status_code,
        request_id,
    } = &attempt.record.disposition
    else {
        panic!("completed stream attempt must carry AttemptDisposition::Succeeded");
    };
    let event = attempt_success_event(
        &AttemptMeta {
            provider: attempt.record.provider_kind,
            model: attempt.record.model.clone(),
            success: true,
            status_code: *status_code,
            request_id: request_id.clone(),
            error_kind: None,
            error_message: None,
        },
        attempt.target_index,
        attempt.attempt_index,
        attempt.started_at.elapsed(),
    );
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
    let event = attempt_failure_event(meta, target_index, attempt_index, started_at.elapsed());
    safe_call_observer(observer, |runtime_observer| {
        runtime_observer.on_attempt_failure(&event);
    });
}

pub(super) fn emit_request_end_success(
    state: &StreamDriverState,
    attempt: &CompletedAttemptContext,
) {
    let AttemptDisposition::Succeeded {
        status_code,
        request_id,
    } = &attempt.record.disposition
    else {
        panic!("completed stream attempt must carry AttemptDisposition::Succeeded");
    };
    let event = request_end_success_event(RequestEndContext {
        request_id: request_id.clone(),
        provider: Some(attempt.record.provider_kind),
        model: Some(attempt.record.model.clone()),
        target_index: Some(attempt.target_index),
        attempt_index: Some(attempt.attempt_index),
        elapsed: state.request_started_at.elapsed(),
        status_code: *status_code,
    });
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
    let event = request_end_failure_event(
        RequestEndContext {
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
        },
        terminal_error.kind,
        terminal_error.message.clone(),
    );
    safe_call_observer(state.request_observer.as_ref(), |observer| {
        observer.on_request_end(&event);
    });
}
