use std::{sync::Arc, time::Duration};

use agent_core::{ProviderInstanceId, ProviderKind};

use crate::attempt::SkipReason;
use crate::{AttemptDisposition, AttemptRecord, RuntimeErrorKind};

/// Best-effort runtime lifecycle observer.
///
/// Observer callbacks are advisory only. Runtime code intentionally suppresses
/// observer panics so instrumentation cannot change request control flow or
/// fail an otherwise successful operation.
pub trait RuntimeObserver: Send + Sync {
    /// Called once when a request begins.
    fn on_request_start(&self, _event: &RequestStartEvent) {}
    /// Called when an individual provider attempt begins.
    fn on_attempt_start(&self, _event: &AttemptStartEvent) {}
    /// Called when an individual provider attempt succeeds.
    fn on_attempt_success(&self, _event: &AttemptSuccessEvent) {}
    /// Called when an individual provider attempt fails.
    fn on_attempt_failure(&self, _event: &AttemptFailureEvent) {}
    /// Called when planning skips an attempt before provider execution begins.
    fn on_attempt_skipped(&self, _event: &AttemptSkippedEvent) {}
    /// Called once when the overall request completes.
    fn on_request_end(&self, _event: &RequestEndEvent) {}
}

/// Observer payload emitted once when a request begins.
///
/// `provider`/`model` describe the initially selected target when known. Routed
/// requests may still fall back to later targets after this event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestStartEvent {
    /// Provider request ID when it is already known at request start.
    pub request_id: Option<String>,
    /// Initially selected provider, if resolution succeeded before dispatch.
    pub provider: Option<ProviderKind>,
    /// Initially selected model after request/target normalization, if any.
    pub model: Option<String>,
    /// Target index for this event. Always `None` for request-start.
    pub target_index: Option<usize>,
    /// Attempt index for this event. Always `None` for request-start.
    pub attempt_index: Option<usize>,
    /// Elapsed wall-clock time since request start.
    pub elapsed: Duration,
    /// First resolved target provider for the request, if any targets exist.
    pub first_target: Option<ProviderKind>,
    /// Total number of resolved targets considered for the request.
    pub resolved_target_count: usize,
}

/// Observer payload emitted when an attempt starts for a specific target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptStartEvent {
    /// Provider request ID when it is already known at attempt start.
    pub request_id: Option<String>,
    /// Provider selected for this attempt.
    pub provider: Option<ProviderKind>,
    /// Model selected for this attempt after normalization, if any.
    pub model: Option<String>,
    /// Zero-based target index for the attempt.
    pub target_index: Option<usize>,
    /// Zero-based attempt index for the attempt.
    pub attempt_index: Option<usize>,
    /// Elapsed wall-clock time since the attempt started.
    pub elapsed: Duration,
}

/// Observer payload emitted when an attempt finishes successfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptSuccessEvent {
    /// Provider request identifier, when available.
    pub request_id: Option<String>,
    /// Provider used for the attempt.
    pub provider: Option<ProviderKind>,
    /// Model selected for the attempt.
    pub model: Option<String>,
    /// Zero-based target index for the attempt.
    pub target_index: Option<usize>,
    /// Zero-based attempt index for the attempt.
    pub attempt_index: Option<usize>,
    /// Elapsed wall-clock time since the attempt started.
    pub elapsed: Duration,
    /// HTTP status code associated with the successful attempt, when available.
    pub status_code: Option<u16>,
}

/// Observer payload emitted when an attempt finishes with an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptFailureEvent {
    /// Provider request identifier, when available.
    pub request_id: Option<String>,
    /// Provider used for the attempt.
    pub provider: Option<ProviderKind>,
    /// Model selected for the attempt.
    pub model: Option<String>,
    /// Zero-based target index for the attempt.
    pub target_index: Option<usize>,
    /// Zero-based attempt index for the attempt.
    pub attempt_index: Option<usize>,
    /// Elapsed wall-clock time since the attempt started.
    pub elapsed: Duration,
    /// High-level error kind, when known.
    pub error_kind: Option<RuntimeErrorKind>,
    /// Human-readable error message, when available.
    pub error_message: Option<String>,
}

/// Observer payload emitted when planning rejects a route attempt before
/// provider execution begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptSkippedEvent {
    /// Registered provider instance selected for the skipped attempt.
    pub provider_instance: ProviderInstanceId,
    /// Concrete provider kind resolved for the skipped attempt.
    pub provider_kind: ProviderKind,
    /// Model selected for the skipped attempt.
    pub model: String,
    /// Zero-based target index for the skipped attempt.
    pub target_index: usize,
    /// Zero-based attempt index for the skipped attempt.
    pub attempt_index: usize,
    /// Elapsed wall-clock time spent planning this attempt.
    pub elapsed: Duration,
    /// Planning-only reason the attempt was skipped.
    pub reason: SkipReason,
}

/// Observer payload emitted once when a request terminates.
///
/// On success, `error_kind` and `error_message` are `None`. On failure,
/// `status_code` may still be present when the provider returned a terminal
/// status before the request failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestEndEvent {
    /// Provider request identifier, when available.
    pub request_id: Option<String>,
    /// Provider selected for the terminal attempt, when known.
    pub provider: Option<ProviderKind>,
    /// Model selected for the terminal attempt, when known.
    pub model: Option<String>,
    /// Zero-based target index for the terminal attempt.
    pub target_index: Option<usize>,
    /// Zero-based attempt index for the terminal attempt.
    pub attempt_index: Option<usize>,
    /// Elapsed wall-clock time since request start.
    pub elapsed: Duration,
    /// HTTP status code returned by the terminal attempt, when available.
    pub status_code: Option<u16>,
    /// Terminal error kind, or `None` on success.
    pub error_kind: Option<RuntimeErrorKind>,
    /// Terminal error message, or `None` on success.
    pub error_message: Option<String>,
}

/// Resolves the observer to use for a request.
///
/// Precedence is explicit and stable: send-level observer overrides toolkit-
/// level observer, which overrides client-level observer.
pub fn resolve_observer_for_request<'a>(
    client_observer: Option<&'a Arc<dyn RuntimeObserver>>,
    toolkit_observer: Option<&'a Arc<dyn RuntimeObserver>>,
    send_observer: Option<&'a Arc<dyn RuntimeObserver>>,
) -> Option<&'a Arc<dyn RuntimeObserver>> {
    send_observer.or(toolkit_observer).or(client_observer)
}

/// Invokes an observer callback while suppressing observer panics.
///
/// This is intentional: observer failures must not alter runtime behavior, and
/// the runtime does not propagate observer panics back to callers.
pub fn safe_call_observer(
    observer: Option<&Arc<dyn RuntimeObserver>>,
    call: impl FnOnce(&dyn RuntimeObserver),
) {
    if let Some(observer) = observer {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            call(observer.as_ref());
        }));
    }
}

pub(crate) fn attempt_start_event(
    provider: ProviderKind,
    model: Option<String>,
    target_index: usize,
    attempt_index: usize,
    elapsed: Duration,
) -> AttemptStartEvent {
    AttemptStartEvent {
        request_id: None,
        provider: Some(provider),
        model,
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed,
    }
}

pub(crate) fn attempt_success_event_fields(
    provider: ProviderKind,
    model: Option<String>,
    request_id: Option<String>,
    target_index: usize,
    attempt_index: usize,
    elapsed: Duration,
    status_code: Option<u16>,
) -> AttemptSuccessEvent {
    AttemptSuccessEvent {
        request_id,
        provider: Some(provider),
        model,
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed,
        status_code,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn attempt_failure_event_fields(
    provider: ProviderKind,
    model: Option<String>,
    request_id: Option<String>,
    target_index: usize,
    attempt_index: usize,
    elapsed: Duration,
    error_kind: Option<RuntimeErrorKind>,
    error_message: Option<String>,
) -> AttemptFailureEvent {
    AttemptFailureEvent {
        request_id,
        provider: Some(provider),
        model,
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed,
        error_kind,
        error_message,
    }
}

pub(crate) fn attempt_skipped_event(
    attempt: &AttemptRecord,
    elapsed: Duration,
) -> AttemptSkippedEvent {
    let AttemptDisposition::Skipped { reason } = &attempt.disposition else {
        unreachable!("attempt_skipped_event requires AttemptDisposition::Skipped");
    };

    AttemptSkippedEvent {
        provider_instance: attempt.provider_instance.clone(),
        provider_kind: attempt.provider_kind,
        model: attempt.model.clone(),
        target_index: attempt.target_index,
        attempt_index: attempt.attempt_index,
        elapsed,
        reason: reason.clone(),
    }
}

pub(crate) fn attempt_success_event(
    attempt: &AttemptRecord,
    elapsed: Duration,
) -> AttemptSuccessEvent {
    let AttemptDisposition::Succeeded {
        status_code,
        request_id,
    } = &attempt.disposition
    else {
        unreachable!("attempt_success_event requires AttemptDisposition::Succeeded");
    };

    attempt_success_event_fields(
        attempt.provider_kind,
        Some(attempt.model.clone()),
        request_id.clone(),
        attempt.target_index,
        attempt.attempt_index,
        elapsed,
        *status_code,
    )
}

pub(crate) fn attempt_failure_event(
    attempt: &AttemptRecord,
    elapsed: Duration,
) -> AttemptFailureEvent {
    let AttemptDisposition::Failed {
        error_kind,
        error_message,
        request_id,
        ..
    } = &attempt.disposition
    else {
        unreachable!("attempt_failure_event requires AttemptDisposition::Failed");
    };

    attempt_failure_event_fields(
        attempt.provider_kind,
        Some(attempt.model.clone()),
        request_id.clone(),
        attempt.target_index,
        attempt.attempt_index,
        elapsed,
        Some(*error_kind),
        Some(error_message.clone()),
    )
}
