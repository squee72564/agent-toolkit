use agent_core::{ProviderInstanceId, ProviderKind};

use crate::routing::AttemptRecord;
use crate::{RuntimeError, RuntimeErrorKind};

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
