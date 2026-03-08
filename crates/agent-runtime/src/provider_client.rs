use std::{collections::BTreeMap, sync::Arc};

use agent_core::{Request, Response};

use crate::message_create_input::MessageCreateInput;
use crate::messages_api::MessagesApi;
use crate::observer::{resolve_observer_for_request, safe_call_observer};
use crate::provider_runtime::{ProviderAttemptOutcome, ProviderRuntime};
use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::types::{
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent,
    RequestStartEvent, ResponseMeta,
};

#[derive(Debug, Clone)]
pub struct ProviderClient {
    pub runtime: Arc<ProviderRuntime>,
}

impl ProviderClient {
    pub fn new(runtime: ProviderRuntime) -> Self {
        Self {
            runtime: Arc::new(runtime),
        }
    }

    pub fn messages(&self) -> MessagesApi<'_> {
        MessagesApi { client: self }
    }

    pub async fn create(&self, input: MessageCreateInput) -> Result<Response, RuntimeError> {
        self.create_with_meta(input)
            .await
            .map(|(response, _)| response)
    }

    pub async fn create_with_meta(
        &self,
        input: MessageCreateInput,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let request =
            input.into_request_with_options(self.runtime.default_model.as_deref(), false)?;
        self.send_with_meta(request).await
    }

    pub async fn send(&self, request: Request) -> Result<Response, RuntimeError> {
        self.send_with_meta(request)
            .await
            .map(|(response, _)| response)
    }

    pub async fn send_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let request_started_at = std::time::Instant::now();
        let observer = resolve_observer_for_request(self.runtime.observer.as_ref(), None, None);
        let request_start_event = RequestStartEvent {
            request_id: None,
            provider: Some(self.runtime.provider),
            model: if request.model_id.is_empty() {
                None
            } else {
                Some(request.model_id.clone())
            },
            target_index: None,
            attempt_index: None,
            elapsed: request_started_at.elapsed(),
            first_target: Some(self.runtime.provider),
            resolved_target_count: 1,
        };
        safe_call_observer(observer, |runtime_observer| {
            runtime_observer.on_request_start(&request_start_event);
        });
        let attempt_started_at = std::time::Instant::now();
        let attempt_start_event = AttemptStartEvent {
            request_id: None,
            provider: Some(self.runtime.provider),
            model: if request.model_id.is_empty() {
                None
            } else {
                Some(request.model_id.clone())
            },
            target_index: Some(0),
            attempt_index: Some(0),
            elapsed: attempt_started_at.elapsed(),
        };
        safe_call_observer(observer, |runtime_observer| {
            runtime_observer.on_attempt_start(&attempt_start_event);
        });

        let attempt = self
            .runtime
            .execute_attempt(request, None, BTreeMap::new())
            .await;

        match attempt {
            ProviderAttemptOutcome::Success { response, meta } => {
                let attempt_success_event = AttemptSuccessEvent {
                    request_id: meta.request_id.clone(),
                    provider: Some(meta.provider),
                    model: Some(meta.model.clone()),
                    target_index: Some(0),
                    attempt_index: Some(0),
                    elapsed: attempt_started_at.elapsed(),
                    status_code: meta.status_code,
                };
                safe_call_observer(observer, |runtime_observer| {
                    runtime_observer.on_attempt_success(&attempt_success_event);
                });
                let response_meta = ResponseMeta {
                    selected_provider: meta.provider,
                    selected_model: meta.model.clone(),
                    status_code: meta.status_code,
                    request_id: meta.request_id.clone(),
                    attempts: vec![meta],
                };
                let request_end_event = RequestEndEvent {
                    request_id: response_meta.request_id.clone(),
                    provider: Some(response_meta.selected_provider),
                    model: Some(response_meta.selected_model.clone()),
                    target_index: Some(0),
                    attempt_index: Some(0),
                    elapsed: request_started_at.elapsed(),
                    status_code: response_meta.status_code,
                    error_kind: None,
                    error_message: None,
                };
                safe_call_observer(observer, |runtime_observer| {
                    runtime_observer.on_request_end(&request_end_event);
                });

                Ok((response, response_meta))
            }
            ProviderAttemptOutcome::Failure { error, meta } => {
                let attempt_failure_event = AttemptFailureEvent {
                    request_id: meta.request_id.clone(),
                    provider: Some(meta.provider),
                    model: Some(meta.model.clone()),
                    target_index: Some(0),
                    attempt_index: Some(0),
                    elapsed: attempt_started_at.elapsed(),
                    error_kind: meta.error_kind,
                    error_message: meta.error_message,
                };
                safe_call_observer(observer, |runtime_observer| {
                    runtime_observer.on_attempt_failure(&attempt_failure_event);
                });

                let terminal_error = terminal_failure_error(&error);
                let request_end_event = RequestEndEvent {
                    request_id: terminal_error.request_id.clone(),
                    provider: terminal_error.provider,
                    model: Some(meta.model),
                    target_index: Some(0),
                    attempt_index: Some(0),
                    elapsed: request_started_at.elapsed(),
                    status_code: terminal_error.status_code,
                    error_kind: Some(terminal_error.kind),
                    error_message: Some(terminal_error.message.clone()),
                };
                safe_call_observer(observer, |runtime_observer| {
                    runtime_observer.on_request_end(&request_end_event);
                });

                Err(error)
            }
        }
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
