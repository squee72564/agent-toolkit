use std::time::Duration;

use agent_core::{ProviderInstanceId, ProviderKind};

use crate::{
    RuntimeError, RuntimeErrorKind,
    attempt::{AttemptDisposition, AttemptRecord},
    observer::{RequestEndEvent, RequestStartEvent},
};

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
    /// Registered provider instance that produced the returned response.
    pub selected_provider_instance: ProviderInstanceId,
    /// Concrete provider kind that produced the returned response.
    pub selected_provider_kind: ProviderKind,
    /// Model that produced the returned response.
    pub selected_model: String,
    /// HTTP status code from the selected response, when available.
    pub status_code: Option<u16>,
    /// Provider request identifier from the selected response, when available.
    pub request_id: Option<String>,
    /// Ordered route-attempt history for the request.
    pub attempts: Vec<AttemptRecord>,
}

/// Metadata describing the terminal executed failure for a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutedFailureMeta {
    /// Registered provider instance selected for the failed attempt.
    pub selected_provider_instance: ProviderInstanceId,
    /// Concrete provider kind selected for the failed attempt.
    pub selected_provider_kind: ProviderKind,
    /// Model that produced the terminal executed failure.
    pub selected_model: String,
    /// HTTP status code from the failed attempt, when available.
    pub status_code: Option<u16>,
    /// Provider request identifier from the failed attempt, when available.
    pub request_id: Option<String>,
    /// Ordered route-attempt history for the failed call.
    pub attempts: Vec<AttemptRecord>,
}

pub(crate) struct RequestEndContext {
    pub(crate) request_id: Option<String>,
    pub(crate) provider: Option<ProviderKind>,
    pub(crate) model: Option<String>,
    pub(crate) target_index: Option<usize>,
    pub(crate) attempt_index: Option<usize>,
    pub(crate) elapsed: Duration,
    pub(crate) status_code: Option<u16>,
}

pub(crate) fn request_start_event(
    provider: Option<ProviderKind>,
    model: Option<String>,
    elapsed: Duration,
    first_target: Option<ProviderKind>,
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
    selected_provider_instance: ProviderInstanceId,
    selected_provider_kind: ProviderKind,
    selected_model: String,
    status_code: Option<u16>,
    request_id: Option<String>,
    attempts: Vec<AttemptRecord>,
) -> ResponseMeta {
    ResponseMeta {
        selected_provider_instance,
        selected_provider_kind,
        selected_model,
        status_code,
        request_id,
        attempts,
    }
}

pub(crate) fn executed_failure_meta(
    selected_provider_instance: ProviderInstanceId,
    selected_provider_kind: ProviderKind,
    selected_model: String,
    status_code: Option<u16>,
    request_id: Option<String>,
    attempts: Vec<AttemptRecord>,
) -> ExecutedFailureMeta {
    ExecutedFailureMeta {
        selected_provider_instance,
        selected_provider_kind,
        selected_model,
        status_code,
        request_id,
        attempts,
    }
}

pub(crate) fn succeeded_attempt_record(
    provider_instance: ProviderInstanceId,
    provider_kind: ProviderKind,
    model: String,
    target_index: usize,
    attempt_index: usize,
    status_code: Option<u16>,
    request_id: Option<String>,
) -> AttemptRecord {
    AttemptRecord {
        provider_instance,
        provider_kind,
        model,
        target_index,
        attempt_index,
        disposition: AttemptDisposition::Succeeded {
            status_code,
            request_id,
        },
    }
}

pub(crate) fn failed_attempt_record(
    provider_instance: ProviderInstanceId,
    provider_kind: ProviderKind,
    model: String,
    target_index: usize,
    attempt_index: usize,
    error: &RuntimeError,
) -> AttemptRecord {
    AttemptRecord {
        provider_instance,
        provider_kind,
        model,
        target_index,
        attempt_index,
        disposition: AttemptDisposition::Failed {
            error_kind: error.kind,
            error_message: error.message.clone(),
            status_code: error.status_code,
            request_id: error.request_id.clone(),
        },
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
