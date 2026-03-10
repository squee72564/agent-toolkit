use std::sync::Arc;
use std::time::Instant;

use agent_core::{ProviderId, Request};

use super::AgentToolkit;
use crate::observer::{RuntimeObserver, resolve_observer_for_request, safe_call_observer};
use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::send_options::SendOptions;
use crate::target::Target;
use crate::types::{
    AttemptFailureEvent, AttemptMeta, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent,
    RequestStartEvent, ResponseMeta,
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
        _request: &Request,
        options: &SendOptions,
    ) -> Result<Self, RuntimeError> {
        let request_started_at = Instant::now();
        let targets = toolkit.resolve_targets(options)?;
        let first_client_observer = targets
            .first()
            .and_then(|target| toolkit.clients.get(&target.provider))
            .and_then(|client| client.runtime.observer.as_ref());
        let request_observer = resolve_observer_for_request(
            first_client_observer,
            toolkit.observer.as_ref(),
            options.observer.as_ref(),
        )
        .cloned();

        Ok(Self {
            request_started_at,
            targets,
            request_observer,
        })
    }

    pub(super) fn emit_request_start(&self, request: &Request) {
        let event = RequestStartEvent {
            request_id: None,
            provider: self.targets.first().map(|target| target.provider),
            model: self
                .targets
                .first()
                .and_then(|target| event_model(target.model.as_deref(), &request.model_id)),
            target_index: None,
            attempt_index: None,
            elapsed: self.request_started_at.elapsed(),
            first_target: self.targets.first().map(|target| target.provider),
            resolved_target_count: self.targets.len(),
        };
        safe_call_observer(self.request_observer.as_ref(), |observer| {
            observer.on_request_start(&event);
        });
    }

    pub(super) fn attempt(
        &self,
        toolkit: &AgentToolkit,
        options: &SendOptions,
        request_model_id: &str,
        target: &Target,
        index: usize,
    ) -> AttemptExecution {
        let observer = toolkit.resolve_attempt_observer(options, target.provider);
        let started_at = Instant::now();
        let event = AttemptStartEvent {
            request_id: None,
            provider: Some(target.provider),
            model: event_model(target.model.as_deref(), request_model_id),
            target_index: Some(index),
            attempt_index: Some(index),
            elapsed: started_at.elapsed(),
        };
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
        let event = RequestEndEvent {
            request_id: terminal_error.request_id.clone(),
            provider: provider.or(terminal_error.provider),
            model,
            target_index,
            attempt_index,
            elapsed: self.request_started_at.elapsed(),
            status_code: terminal_error.status_code,
            error_kind: Some(terminal_error.kind),
            error_message: Some(terminal_error.message.clone()),
        };
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
        let event = RequestEndEvent {
            request_id,
            provider,
            model,
            target_index,
            attempt_index,
            elapsed: self.request_started_at.elapsed(),
            status_code,
            error_kind: None,
            error_message: None,
        };
        safe_call_observer(self.request_observer.as_ref(), |observer| {
            observer.on_request_end(&event);
        });
    }

    pub(super) fn emit_terminal_request_end(
        &self,
        toolkit: &AgentToolkit,
        options: &SendOptions,
        attempts: &[AttemptMeta],
        error: &RuntimeError,
    ) {
        let terminal_error = terminal_failure_error(error);
        let terminal_provider = terminal_error
            .provider
            .or_else(|| attempts.last().map(|attempt| attempt.provider));
        let request_observer = terminal_provider
            .and_then(|provider| toolkit.resolve_attempt_observer(options, provider))
            .or_else(|| self.request_observer.clone());
        let terminal_index = attempts.len().checked_sub(1);
        let event = RequestEndEvent {
            request_id: terminal_error.request_id.clone(),
            provider: terminal_provider,
            model: attempts.last().map(|attempt| attempt.model.clone()),
            target_index: terminal_index,
            attempt_index: terminal_index,
            elapsed: self.request_started_at.elapsed(),
            status_code: terminal_error.status_code,
            error_kind: Some(terminal_error.kind),
            error_message: Some(terminal_error.message.clone()),
        };
        safe_call_observer(request_observer.as_ref(), |observer| {
            observer.on_request_end(&event);
        });
    }
}

impl AttemptExecution {
    pub(super) fn emit_success(&self, meta: &crate::types::AttemptMeta, index: usize) {
        let event = AttemptSuccessEvent {
            request_id: meta.request_id.clone(),
            provider: Some(meta.provider),
            model: Some(meta.model.clone()),
            target_index: Some(index),
            attempt_index: Some(index),
            elapsed: self.started_at.elapsed(),
            status_code: meta.status_code,
        };
        safe_call_observer(self.observer.as_ref(), |runtime_observer| {
            runtime_observer.on_attempt_success(&event);
        });
    }

    pub(super) fn emit_failure(&self, meta: &crate::types::AttemptMeta, index: usize) {
        let event = AttemptFailureEvent {
            request_id: meta.request_id.clone(),
            provider: Some(meta.provider),
            model: Some(meta.model.clone()),
            target_index: Some(index),
            attempt_index: Some(index),
            elapsed: self.started_at.elapsed(),
            error_kind: meta.error_kind,
            error_message: meta.error_message.clone(),
        };
        safe_call_observer(self.observer.as_ref(), |runtime_observer| {
            runtime_observer.on_attempt_failure(&event);
        });
    }

    pub(super) fn response_meta(
        &self,
        attempts: Vec<AttemptMeta>,
        meta: crate::types::AttemptMeta,
    ) -> ResponseMeta {
        ResponseMeta {
            selected_provider: meta.provider,
            selected_model: meta.model.clone(),
            status_code: meta.status_code,
            request_id: meta.request_id.clone(),
            attempts,
        }
    }
}

pub(super) fn event_model(target_model: Option<&str>, request_model: &str) -> Option<String> {
    target_model
        .and_then(trimmed_non_empty)
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
