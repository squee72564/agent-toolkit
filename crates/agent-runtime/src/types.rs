use std::time::Duration;

use agent_core::{ProviderId, ProviderInstanceId, ProviderKind};

use crate::{RuntimeError, RuntimeErrorKind};

/// Observer payload emitted once when a request begins.
///
/// `provider`/`model` describe the initially selected target when known. Routed
/// requests may still fall back to later targets after this event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestStartEvent {
    /// Provider request ID when it is already known at request start.
    pub request_id: Option<String>,
    /// Initially selected provider, if resolution succeeded before dispatch.
    pub provider: Option<ProviderId>,
    /// Initially selected model after request/target normalization, if any.
    pub model: Option<String>,
    /// Target index for this event. Always `None` for request-start.
    pub target_index: Option<usize>,
    /// Attempt index for this event. Always `None` for request-start.
    pub attempt_index: Option<usize>,
    /// Elapsed wall-clock time since request start.
    pub elapsed: Duration,
    /// First resolved target provider for the request, if any targets exist.
    pub first_target: Option<ProviderId>,
    /// Total number of resolved targets considered for the request.
    pub resolved_target_count: usize,
}

/// Observer payload emitted when an attempt starts for a specific target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptStartEvent {
    /// Provider request ID when it is already known at attempt start.
    pub request_id: Option<String>,
    /// Provider selected for this attempt.
    pub provider: Option<ProviderId>,
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
    pub provider: Option<ProviderId>,
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
    pub provider: Option<ProviderId>,
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
    pub provider: Option<ProviderId>,
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

/// Metadata captured for a single provider attempt.
///
/// This is used both for returned [`ResponseMeta`] and for observer event
/// payload construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptMeta {
    /// Provider used for the attempt.
    pub provider: ProviderId,
    /// Model selected for the attempt.
    pub model: String,
    /// Whether the attempt succeeded.
    pub success: bool,
    /// HTTP status code returned by the provider, when available.
    pub status_code: Option<u16>,
    /// Provider request identifier, when available.
    pub request_id: Option<String>,
    /// Error kind for failed attempts.
    pub error_kind: Option<RuntimeErrorKind>,
    /// Error message for failed attempts.
    pub error_message: Option<String>,
}

/// Planning-only reason for skipping a candidate route attempt before
/// provider execution begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    StaticIncompatibility { message: String },
    AdapterPlanningRejected { message: String },
}

/// Route-attempt disposition shared by planning-failure and execution-history
/// surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptDisposition {
    Skipped {
        reason: SkipReason,
    },
    Succeeded {
        status_code: Option<u16>,
        request_id: Option<String>,
    },
    Failed {
        error_kind: RuntimeErrorKind,
        error_message: String,
        status_code: Option<u16>,
        request_id: Option<String>,
    },
}

/// Ordered route-attempt history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    pub provider_instance: ProviderInstanceId,
    pub provider_kind: ProviderKind,
    pub model: String,
    pub target_index: usize,
    pub attempt_index: usize,
    pub disposition: AttemptDisposition,
}

/// Planning-only route failure emitted when routing terminates before any
/// attempt executes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlanningFailure {
    pub reason: RoutePlanningFailureReason,
    pub attempts: Vec<AttemptRecord>,
}

/// Distinguishes pure static incompatibility exhaustion from adapter-planning
/// rejections that occurred before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePlanningFailureReason {
    NoCompatibleAttempts,
    AllAttemptsRejectedDuringPlanning,
}

impl std::fmt::Display for RoutePlanningFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.reason {
            RoutePlanningFailureReason::NoCompatibleAttempts => {
                "no compatible route attempts remained during planning"
            }
            RoutePlanningFailureReason::AllAttemptsRejectedDuringPlanning => {
                "all route attempts were rejected during planning"
            }
        };

        write!(f, "{message}")
    }
}

impl std::error::Error for RoutePlanningFailure {}

/// Returned metadata describing the selected response and all attempted targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseMeta {
    /// Provider that produced the returned response.
    pub selected_provider: ProviderId,
    /// Model that produced the returned response.
    pub selected_model: String,
    /// HTTP status code from the selected response, when available.
    pub status_code: Option<u16>,
    /// Provider request identifier from the selected response, when available.
    pub request_id: Option<String>,
    /// Attempt metadata in execution order.
    pub attempts: Vec<AttemptMeta>,
}

pub(crate) struct RequestEndContext {
    pub(crate) request_id: Option<String>,
    pub(crate) provider: Option<ProviderId>,
    pub(crate) model: Option<String>,
    pub(crate) target_index: Option<usize>,
    pub(crate) attempt_index: Option<usize>,
    pub(crate) elapsed: Duration,
    pub(crate) status_code: Option<u16>,
}

pub(crate) fn request_start_event(
    provider: Option<ProviderId>,
    model: Option<String>,
    elapsed: Duration,
    first_target: Option<ProviderId>,
    resolved_target_count: usize,
) -> RequestStartEvent {
    RequestStartEvent {
        request_id: None,
        provider,
        model,
        target_index: None,
        attempt_index: None,
        elapsed,
        first_target,
        resolved_target_count,
    }
}

pub(crate) fn attempt_start_event(
    provider: ProviderId,
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

pub(crate) fn attempt_success_event(
    meta: &AttemptMeta,
    target_index: usize,
    attempt_index: usize,
    elapsed: Duration,
) -> AttemptSuccessEvent {
    AttemptSuccessEvent {
        request_id: meta.request_id.clone(),
        provider: Some(meta.provider),
        model: Some(meta.model.clone()),
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed,
        status_code: meta.status_code,
    }
}

pub(crate) fn attempt_failure_event(
    meta: &AttemptMeta,
    target_index: usize,
    attempt_index: usize,
    elapsed: Duration,
) -> AttemptFailureEvent {
    AttemptFailureEvent {
        request_id: meta.request_id.clone(),
        provider: Some(meta.provider),
        model: Some(meta.model.clone()),
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed,
        error_kind: meta.error_kind,
        error_message: meta.error_message.clone(),
    }
}

pub(crate) fn request_end_success_event(context: RequestEndContext) -> RequestEndEvent {
    RequestEndEvent {
        request_id: context.request_id,
        provider: context.provider,
        model: context.model,
        target_index: context.target_index,
        attempt_index: context.attempt_index,
        elapsed: context.elapsed,
        status_code: context.status_code,
        error_kind: None,
        error_message: None,
    }
}

pub(crate) fn request_end_failure_event(
    context: RequestEndContext,
    error_kind: RuntimeErrorKind,
    error_message: String,
) -> RequestEndEvent {
    RequestEndEvent {
        request_id: context.request_id,
        provider: context.provider,
        model: context.model,
        target_index: context.target_index,
        attempt_index: context.attempt_index,
        elapsed: context.elapsed,
        status_code: context.status_code,
        error_kind: Some(error_kind),
        error_message: Some(error_message),
    }
}

pub(crate) fn response_meta(
    selected_provider: ProviderId,
    selected_model: String,
    status_code: Option<u16>,
    request_id: Option<String>,
    attempts: Vec<AttemptMeta>,
) -> ResponseMeta {
    ResponseMeta {
        selected_provider,
        selected_model,
        status_code,
        request_id,
        attempts,
    }
}

pub(crate) fn normalized_event_model(
    target_model: Option<&str>,
    request_model: &str,
) -> Option<String> {
    target_model
        .and_then(trimmed_non_empty)
        .or_else(|| trimmed_non_empty(request_model))
        .map(ToString::to_string)
}

pub(crate) fn terminal_failure_error(error: &RuntimeError) -> &RuntimeError {
    if error.kind == RuntimeErrorKind::FallbackExhausted
        && let Some(source) = error.source_ref()
        && let Some(terminal_error) = source.downcast_ref::<RuntimeError>()
    {
        return terminal_error;
    }
    error
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
