use std::sync::Arc;
use std::time::Instant;

use agent_core::ProviderId;

use super::AgentToolkit;
use crate::execution_options::ExecutionOptions;
use crate::observer::{RuntimeObserver, resolve_observer_for_request, safe_call_observer};
use crate::route::Route;
use crate::runtime_error::RuntimeError;
use crate::target::Target;
use crate::types::{
    AttemptMeta, RequestEndContext, ResponseMeta, attempt_failure_event, attempt_start_event,
    attempt_success_event, normalized_event_model, request_end_failure_event,
    request_end_success_event, request_start_event, response_meta, terminal_failure_error,
};

pub(super) struct PreparedExecution {
    pub(super) request_started_at: Instant,
    pub(super) targets: Vec<Target>,
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
        let targets = toolkit.resolve_route_targets(route)?;
        let first_client_observer = targets
            .first()
            .and_then(|target| toolkit.clients.get(&target.instance))
            .and_then(|client| client.runtime.observer.as_ref());
        let request_observer = resolve_observer_for_request(
            first_client_observer,
            toolkit.observer.as_ref(),
            execution.observer.as_ref(),
        )
        .cloned();

        Ok(Self {
            request_started_at,
            targets,
            request_observer,
        })
    }

    pub(super) fn emit_request_start(&self, request_model: Option<&str>) {
        let event = request_start_event(
            None,
            self.targets
                .first()
                .and_then(|target| event_model(target.model.as_deref(), request_model)),
            self.request_started_at.elapsed(),
            None,
            self.targets.len(),
        );
        safe_call_observer(self.request_observer.as_ref(), |observer| {
            observer.on_request_start(&event);
        });
    }

    pub(super) fn attempt(
        &self,
        toolkit: &AgentToolkit,
        execution: &ExecutionOptions,
        request_model: Option<&str>,
        target: &Target,
        index: usize,
    ) -> AttemptExecution {
        let observer = toolkit.resolve_attempt_observer(execution, target.instance.clone());
        let provider = toolkit.clients[&target.instance].runtime.kind;
        let started_at = Instant::now();
        let event = attempt_start_event(
            provider,
            event_model(target.model.as_deref(), request_model),
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
        provider: Option<ProviderId>,
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
        provider: Option<ProviderId>,
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
        attempts: &[AttemptMeta],
        error: &RuntimeError,
    ) {
        let terminal_error = terminal_failure_error(error);
        let terminal_provider = terminal_error
            .provider
            .or_else(|| attempts.last().map(|attempt| attempt.provider));
        let request_observer = self.request_observer.clone();
        let terminal_index = attempts.len().checked_sub(1);
        let event = request_end_failure_event(
            RequestEndContext {
                request_id: terminal_error.request_id.clone(),
                provider: terminal_provider,
                model: attempts.last().map(|attempt| attempt.model.clone()),
                target_index: terminal_index,
                attempt_index: terminal_index,
                elapsed: self.request_started_at.elapsed(),
                status_code: terminal_error.status_code,
            },
            terminal_error.kind,
            terminal_error.message.clone(),
        );
        safe_call_observer(request_observer.as_ref(), |observer| {
            observer.on_request_end(&event);
        });
    }
}

impl AttemptExecution {
    pub(super) fn emit_success(&self, meta: &crate::types::AttemptMeta, index: usize) {
        let event = attempt_success_event(meta, index, index, self.started_at.elapsed());
        safe_call_observer(self.observer.as_ref(), |runtime_observer| {
            runtime_observer.on_attempt_success(&event);
        });
    }

    pub(super) fn emit_failure(&self, meta: &crate::types::AttemptMeta, index: usize) {
        let event = attempt_failure_event(meta, index, index, self.started_at.elapsed());
        safe_call_observer(self.observer.as_ref(), |runtime_observer| {
            runtime_observer.on_attempt_failure(&event);
        });
    }

    pub(super) fn response_meta(
        &self,
        attempts: Vec<AttemptMeta>,
        meta: crate::types::AttemptMeta,
    ) -> ResponseMeta {
        response_meta(
            meta.provider,
            meta.model.clone(),
            meta.status_code,
            meta.request_id.clone(),
            attempts,
        )
    }
}

pub(super) fn event_model(
    target_model: Option<&str>,
    request_model: Option<&str>,
) -> Option<String> {
    normalized_event_model(target_model, request_model.unwrap_or_default())
}
