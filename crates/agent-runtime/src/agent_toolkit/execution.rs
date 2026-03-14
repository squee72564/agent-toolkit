use std::sync::Arc;
use std::time::Instant;

use agent_core::ProviderKind;

use super::AgentToolkit;
use crate::attempt_spec::AttemptSpec;
use crate::execution_options::ExecutionOptions;
use crate::observer::{RuntimeObserver, resolve_observer_for_request, safe_call_observer};
use crate::planner::SkippedPlannedAttempt;
use crate::route::Route;
use crate::runtime_error::RuntimeError;
use crate::target::Target;
use crate::types::{
    AttemptDisposition, AttemptRecord, RequestEndContext, attempt_failure_event_fields,
    attempt_skipped_event, attempt_start_event, attempt_success_event_fields,
    normalized_event_model, request_end_failure_event, request_end_success_event,
    request_start_event, terminal_failure_error,
};

pub(super) struct PreparedExecution {
    pub(super) request_started_at: Instant,
    pub(super) attempts: Vec<AttemptSpec>,
    pub(super) request_observer: Option<Arc<dyn RuntimeObserver>>,
}

pub(super) struct AttemptExecution {
    pub(super) observer: Option<Arc<dyn RuntimeObserver>>,
    pub(super) started_at: Instant,
}

impl PreparedExecution {
    pub(super) fn new(
        toolkit: &AgentToolkit,
        route: &Route,
        execution: &ExecutionOptions,
    ) -> Result<Self, RuntimeError> {
        let request_started_at = Instant::now();
        let attempts = toolkit.resolve_route_targets(route)?;
        let first_client_observer = attempts
            .first()
            .and_then(|attempt| toolkit.clients.get(&attempt.target.instance))
            .and_then(|client| client.runtime.observer.as_ref());
        let request_observer = resolve_observer_for_request(
            first_client_observer,
            toolkit.observer.as_ref(),
            execution.observer.as_ref(),
        )
        .cloned();

        Ok(Self {
            request_started_at,
            attempts,
            request_observer,
        })
    }

    pub(super) fn emit_request_start(&self, request_model: Option<&str>) {
        let event = request_start_event(
            None,
            self.attempts
                .first()
                .and_then(|attempt| event_model(attempt.target.model.as_deref(), request_model)),
            self.request_started_at.elapsed(),
            None,
            self.attempts.len(),
        );
        safe_call_observer(self.request_observer.as_ref(), |observer| {
            observer.on_request_start(&event);
        });
    }

    pub(super) fn attempt(
        &self,
        toolkit: &AgentToolkit,
        execution: &ExecutionOptions,
        effective_model: Option<&str>,
        target: &Target,
        index: usize,
    ) -> AttemptExecution {
        let observer = toolkit.resolve_attempt_observer(execution, target.instance.clone());
        let provider = toolkit.clients[&target.instance].runtime.kind;
        let started_at = Instant::now();
        let event = attempt_start_event(
            provider,
            effective_model.map(ToString::to_string),
            index,
            index,
            started_at.elapsed(),
        );
        safe_call_observer(observer.as_ref(), |runtime_observer| {
            runtime_observer.on_attempt_start(&event);
        });

        AttemptExecution {
            observer,
            started_at,
        }
    }

    pub(super) fn emit_request_end_failure(
        &self,
        provider: Option<ProviderKind>,
        model: Option<String>,
        target_index: Option<usize>,
        attempt_index: Option<usize>,
        error: &RuntimeError,
    ) {
        let terminal_error = terminal_failure_error(error);
        let event = request_end_failure_event(
            RequestEndContext {
                request_id: terminal_error.request_id.clone(),
                provider: provider.or(terminal_error.provider),
                model,
                target_index,
                attempt_index,
                elapsed: self.request_started_at.elapsed(),
                status_code: terminal_error.status_code,
            },
            terminal_error.kind,
            terminal_error.message.clone(),
        );
        safe_call_observer(self.request_observer.as_ref(), |observer| {
            observer.on_request_end(&event);
        });
    }

    pub(super) fn emit_request_end_success(
        &self,
        provider: Option<ProviderKind>,
        model: Option<String>,
        target_index: Option<usize>,
        attempt_index: Option<usize>,
        request_id: Option<String>,
        status_code: Option<u16>,
    ) {
        let event = request_end_success_event(RequestEndContext {
            request_id,
            provider,
            model,
            target_index,
            attempt_index,
            elapsed: self.request_started_at.elapsed(),
            status_code,
        });
        safe_call_observer(self.request_observer.as_ref(), |observer| {
            observer.on_request_end(&event);
        });
    }

    pub(super) fn emit_terminal_request_end(
        &self,
        _toolkit: &AgentToolkit,
        _execution: &ExecutionOptions,
        attempts: &[AttemptRecord],
        error: &RuntimeError,
    ) {
        let terminal_error = terminal_failure_error(error);
        let terminal_attempt = attempts.iter().rev().find(|attempt| {
            matches!(
                attempt.disposition,
                AttemptDisposition::Succeeded { .. } | AttemptDisposition::Failed { .. }
            )
        });
        let event = request_end_failure_event(
            RequestEndContext {
                request_id: terminal_error.request_id.clone(),
                provider: terminal_error
                    .provider
                    .or_else(|| terminal_attempt.map(|attempt| attempt.provider_kind)),
                model: terminal_attempt.map(|attempt| attempt.model.clone()),
                target_index: terminal_attempt.map(|attempt| attempt.target_index),
                attempt_index: terminal_attempt.map(|attempt| attempt.attempt_index),
                elapsed: self.request_started_at.elapsed(),
                status_code: terminal_error.status_code,
            },
            terminal_error.kind,
            terminal_error.message.clone(),
        );
        safe_call_observer(self.request_observer.as_ref(), |observer| {
            observer.on_request_end(&event);
        });
    }

    pub(super) fn emit_attempt_skipped(
        &self,
        toolkit: &AgentToolkit,
        execution: &ExecutionOptions,
        skipped: &SkippedPlannedAttempt,
    ) {
        let observer =
            toolkit.resolve_attempt_observer(execution, skipped.record.provider_instance.clone());
        let event = attempt_skipped_event(&skipped.record, skipped.elapsed);
        safe_call_observer(observer.as_ref(), |runtime_observer| {
            runtime_observer.on_attempt_skipped(&event);
        });
    }
}

impl AttemptExecution {
    pub(super) fn emit_success(&self, meta: &crate::types::AttemptMeta, index: usize) {
        let event = attempt_success_event_fields(
            meta.provider,
            Some(meta.model.clone()),
            meta.request_id.clone(),
            index,
            index,
            self.started_at.elapsed(),
            meta.status_code,
        );
        safe_call_observer(self.observer.as_ref(), |runtime_observer| {
            runtime_observer.on_attempt_success(&event);
        });
    }

    pub(super) fn emit_failure(&self, meta: &crate::types::AttemptMeta, index: usize) {
        let event = attempt_failure_event_fields(
            meta.provider,
            Some(meta.model.clone()),
            meta.request_id.clone(),
            index,
            index,
            self.started_at.elapsed(),
            meta.error_kind,
            meta.error_message.clone(),
        );
        safe_call_observer(self.observer.as_ref(), |runtime_observer| {
            runtime_observer.on_attempt_failure(&event);
        });
    }
}

pub(super) fn event_model(
    target_model: Option<&str>,
    request_model: Option<&str>,
) -> Option<String> {
    normalized_event_model(target_model, request_model.unwrap_or_default())
}
