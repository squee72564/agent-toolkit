use std::{sync::Arc, time::Instant};

use agent_core::{Response, TaskRequest};

use crate::direct_messages_api::DirectMessagesApi;
use crate::direct_streaming_api::DirectStreamingApi;
use crate::execution_options::{ExecutionOptions, ResponseMode};
use crate::message_create_input::MessageCreateInput;
use crate::message_response_stream::{LiveAttempt, MessageResponseStream};
use crate::observer::{resolve_observer_for_request, safe_call_observer};
use crate::planner;
use crate::provider_runtime::{
    ProviderAttemptOutcome, ProviderRuntime, ProviderStreamAttemptOutcome,
};
use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::types::{
    RequestEndContext, ResponseMeta, attempt_failure_event, attempt_start_event,
    attempt_success_event, request_end_failure_event, request_end_success_event,
    request_start_event, response_meta, terminal_failure_error,
};

#[derive(Debug, Clone)]
pub(crate) struct ProviderClient {
    pub(crate) runtime: Arc<ProviderRuntime>,
}

struct DirectRequestContext<'a> {
    request_started_at: Instant,
    attempt_started_at: Instant,
    observer: Option<&'a Arc<dyn crate::observer::RuntimeObserver>>,
    request_model: Option<String>,
}

struct DirectFailureContext {
    request_id: Option<String>,
    provider: Option<agent_core::ProviderId>,
    model: Option<String>,
    status_code: Option<u16>,
    error_kind: RuntimeErrorKind,
    error_message: String,
}

impl ProviderClient {
    pub(crate) fn new(runtime: ProviderRuntime) -> Self {
        Self {
            runtime: Arc::new(runtime),
        }
    }

    pub fn messages(&self) -> DirectMessagesApi<'_> {
        DirectMessagesApi::new(self)
    }

    pub fn streaming(&self) -> DirectStreamingApi<'_> {
        DirectStreamingApi::new(self)
    }

    pub(crate) async fn create(&self, input: MessageCreateInput) -> Result<Response, RuntimeError> {
        self.create_with_meta(input)
            .await
            .map(|(response, _)| response)
    }

    pub(crate) async fn create_with_meta(
        &self,
        input: MessageCreateInput,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        if input.is_streaming() {
            return Err(RuntimeError::configuration(
                "stream=true is not supported by the current messages/send response API yet",
            ));
        }
        let (task, model_override, execution) = input.into_task_request_parts()?;
        self.execute_with_meta(task, model_override, execution)
            .await
    }

    pub(crate) async fn execute(
        &self,
        task: TaskRequest,
        model_override: Option<String>,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.execute_with_meta(task, model_override, execution)
            .await
            .map(|(response, _)| response)
    }

    pub(crate) async fn execute_with_meta(
        &self,
        task: TaskRequest,
        model_override: Option<String>,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        if execution.response_mode != ResponseMode::NonStreaming {
            return Err(RuntimeError::configuration(
                "messages() requires ExecutionOptions.response_mode = ResponseMode::NonStreaming",
            ));
        }
        let execution_plan =
            planner::plan_direct_attempt(self, &task, model_override.as_deref(), &execution)?;
        let context = self.begin_direct_request(execution_plan.attempt.model.as_str());

        let attempt = self.runtime.execute_attempt(execution_plan).await;

        match attempt {
            ProviderAttemptOutcome::Success { response, meta } => {
                self.emit_attempt_success(&context, &meta);
                let response_meta = response_meta(
                    meta.provider,
                    meta.model.clone(),
                    meta.status_code,
                    meta.request_id.clone(),
                    vec![meta],
                );
                self.emit_request_end_success(&context, &response_meta);

                Ok((response, response_meta))
            }
            ProviderAttemptOutcome::Failure { error, meta } => {
                self.emit_attempt_failure(&context, &meta);
                let terminal_error = terminal_failure_error(&error);
                self.emit_request_end_failure(
                    &context,
                    DirectFailureContext {
                        request_id: terminal_error.request_id.clone(),
                        provider: terminal_error.provider,
                        model: Some(meta.model),
                        status_code: terminal_error.status_code,
                        error_kind: terminal_error.kind,
                        error_message: terminal_error.message.clone(),
                    },
                );

                Err(error)
            }
        }
    }

    pub(crate) async fn create_stream(
        &self,
        input: MessageCreateInput,
    ) -> Result<MessageResponseStream, RuntimeError> {
        let (task, model_override, mut execution) = input.into_task_request_parts()?;
        execution.response_mode = ResponseMode::Streaming;
        self.execute_stream(task, model_override, execution).await
    }

    pub(crate) async fn execute_stream(
        &self,
        task: TaskRequest,
        model_override: Option<String>,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        if execution.response_mode != ResponseMode::Streaming {
            return Err(RuntimeError::configuration(
                "streaming() requires ExecutionOptions.response_mode = ResponseMode::Streaming",
            ));
        }
        let execution_plan =
            planner::plan_direct_attempt(self, &task, model_override.as_deref(), &execution)?;
        let context = self.begin_direct_request(execution_plan.attempt.model.as_str());
        let stream_observer = context.cloned_observer();

        match self
            .runtime
            .open_stream_attempt(execution_plan)
            .await
        {
            ProviderStreamAttemptOutcome::Opened { stream, meta } => {
                Ok(MessageResponseStream::new_direct(
                    context.request_started_at,
                    stream_observer,
                    LiveAttempt {
                        stream: *stream,
                        context: crate::message_response_stream::AttemptContext {
                            target_index: 0,
                            attempt_index: 0,
                            started_at: context.attempt_started_at,
                            observer: context.cloned_observer(),
                            provider: meta.provider,
                            model: meta.model,
                            request_id: meta.request_id,
                            status_code: meta.status_code,
                        },
                    },
                ))
            }
            ProviderStreamAttemptOutcome::Failure { error, meta } => {
                self.emit_attempt_failure(&context, &meta);
                self.emit_request_end_failure(
                    &context,
                    DirectFailureContext {
                        request_id: error.request_id.clone(),
                        provider: Some(meta.provider),
                        model: Some(meta.model),
                        status_code: error.status_code,
                        error_kind: error.kind,
                        error_message: error.message.clone(),
                    },
                );
                Err(error)
            }
        }
    }

    fn begin_direct_request<'a>(&'a self, model_id: &str) -> DirectRequestContext<'a> {
        let request_started_at = Instant::now();
        let observer = resolve_observer_for_request(self.runtime.observer.as_ref(), None, None);
        let request_model = request_model(model_id);
        let request_start_event = request_start_event(
            Some(self.runtime.kind),
            request_model.clone(),
            request_started_at.elapsed(),
            Some(self.runtime.kind),
            1,
        );
        safe_call_observer(observer, |runtime_observer| {
            runtime_observer.on_request_start(&request_start_event);
        });

        let attempt_started_at = Instant::now();
        let attempt_start_event = attempt_start_event(
            self.runtime.kind,
            request_model.clone(),
            0,
            0,
            attempt_started_at.elapsed(),
        );
        safe_call_observer(observer, |runtime_observer| {
            runtime_observer.on_attempt_start(&attempt_start_event);
        });

        DirectRequestContext {
            request_started_at,
            attempt_started_at,
            observer,
            request_model,
        }
    }

    fn emit_attempt_success(
        &self,
        context: &DirectRequestContext<'_>,
        meta: &crate::types::AttemptMeta,
    ) {
        let event = attempt_success_event(meta, 0, 0, context.attempt_started_at.elapsed());
        safe_call_observer(context.observer, |runtime_observer| {
            runtime_observer.on_attempt_success(&event);
        });
    }

    fn emit_attempt_failure(
        &self,
        context: &DirectRequestContext<'_>,
        meta: &crate::types::AttemptMeta,
    ) {
        let event = attempt_failure_event(meta, 0, 0, context.attempt_started_at.elapsed());
        safe_call_observer(context.observer, |runtime_observer| {
            runtime_observer.on_attempt_failure(&event);
        });
    }

    fn emit_request_end_success(
        &self,
        context: &DirectRequestContext<'_>,
        response_meta: &ResponseMeta,
    ) {
        let event = request_end_success_event(RequestEndContext {
            request_id: response_meta.request_id.clone(),
            provider: Some(response_meta.selected_provider),
            model: Some(response_meta.selected_model.clone()),
            target_index: Some(0),
            attempt_index: Some(0),
            elapsed: context.request_started_at.elapsed(),
            status_code: response_meta.status_code,
        });
        safe_call_observer(context.observer, |runtime_observer| {
            runtime_observer.on_request_end(&event);
        });
    }

    fn emit_request_end_failure(
        &self,
        context: &DirectRequestContext<'_>,
        failure: DirectFailureContext,
    ) {
        let event = request_end_failure_event(
            RequestEndContext {
                request_id: failure.request_id,
                provider: failure.provider,
                model: failure.model.or_else(|| context.request_model.clone()),
                target_index: Some(0),
                attempt_index: Some(0),
                elapsed: context.request_started_at.elapsed(),
                status_code: failure.status_code,
            },
            failure.error_kind,
            failure.error_message,
        );
        safe_call_observer(context.observer, |runtime_observer| {
            runtime_observer.on_request_end(&event);
        });
    }
}

impl DirectRequestContext<'_> {
    fn cloned_observer(&self) -> Option<Arc<dyn crate::observer::RuntimeObserver>> {
        self.observer.cloned()
    }
}

fn request_model(model_id: &str) -> Option<String> {
    (!model_id.is_empty()).then(|| model_id.to_string())
}
