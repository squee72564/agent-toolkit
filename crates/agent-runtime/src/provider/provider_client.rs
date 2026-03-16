use std::{sync::Arc, time::Instant};

use agent_core::{Response, TaskRequest};

use crate::api::{DirectMessagesApi, DirectStreamingApi};
use crate::execution_options::{ExecutionOptions, ResponseMode};
use crate::message::MessageCreateInput;
use crate::message_response_stream::{AttemptContext, LiveAttempt, MessageResponseStream};
use crate::observability::{
    RequestEndContext, RuntimeObserver, attempt_failure_event, attempt_start_event,
    attempt_success_event, request_end_failure_event, request_end_success_event,
    request_start_event, resolve_observer_for_request, safe_call_observer,
};
use crate::provider_runtime::{
    ProviderAttemptOutcome, ProviderRuntime, ProviderStreamAttemptOutcome,
};

use crate::routing::{failed_attempt_record, succeeded_attempt_record};
use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::types::{ResponseMeta, executed_failure_meta, response_meta, terminal_failure_error};
use crate::{
    Target,
    routing::{
        attempt::{AttemptRecord, AttemptSpec},
        planner,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ProviderClient {
    pub(crate) runtime: Arc<ProviderRuntime>,
}

struct DirectRequestContext<'a> {
    request_started_at: Instant,
    attempt_started_at: Instant,
    observer: Option<&'a Arc<dyn RuntimeObserver>>,
    request_model: Option<String>,
}

struct DirectFailureContext {
    request_id: Option<String>,
    provider: Option<agent_core::ProviderKind>,
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
        self.execute_with_meta(input.into_task_request()?, ExecutionOptions::default())
            .await
    }

    pub(crate) async fn execute(
        &self,
        task: TaskRequest,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.execute_with_meta(task, execution)
            .await
            .map(|(response, _)| response)
    }

    pub(crate) async fn execute_with_meta(
        &self,
        task: TaskRequest,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.execute_on_attempt_with_meta(task, self.default_attempt(), execution)
            .await
    }

    pub(crate) async fn execute_on_attempt(
        &self,
        task: TaskRequest,
        attempt: AttemptSpec,
        execution: ExecutionOptions,
    ) -> Result<Response, RuntimeError> {
        self.execute_on_attempt_with_meta(task, attempt, execution)
            .await
            .map(|(response, _)| response)
    }

    pub(crate) async fn execute_on_attempt_with_meta(
        &self,
        task: TaskRequest,
        attempt: AttemptSpec,
        execution: ExecutionOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        if execution.response_mode != ResponseMode::NonStreaming {
            return Err(RuntimeError::configuration(
                "messages() requires ExecutionOptions.response_mode = ResponseMode::NonStreaming",
            ));
        }
        let execution_plan = planner::plan_direct_attempt(self, &task, &attempt, &execution)?;
        let context = self.begin_direct_request(execution_plan.provider_attempt.model.as_str());

        let attempt = self.runtime.execute_attempt(execution_plan).await;

        match attempt {
            ProviderAttemptOutcome::Success {
                response,
                selected_model,
                status_code,
                request_id,
            } => {
                let attempt_record = succeeded_attempt_record(
                    self.runtime.instance_id.clone(),
                    self.runtime.kind,
                    selected_model.clone(),
                    0,
                    0,
                    status_code,
                    request_id.clone(),
                );
                self.emit_attempt_success(&context, &attempt_record);
                let response_meta = response_meta(
                    self.runtime.instance_id.clone(),
                    self.runtime.kind,
                    selected_model,
                    status_code,
                    request_id,
                    vec![attempt_record],
                );
                self.emit_request_end_success(&context, &response_meta);

                Ok((response, response_meta))
            }
            ProviderAttemptOutcome::Failure {
                error,
                selected_model,
            } => {
                let attempt_record = failed_attempt_record(
                    self.runtime.instance_id.clone(),
                    self.runtime.kind,
                    selected_model.clone(),
                    0,
                    0,
                    &error,
                );
                self.emit_attempt_failure(&context, &attempt_record);
                let status_code = error.status_code;
                let request_id = error.request_id.clone();
                let error = error.with_executed_failure_meta(executed_failure_meta(
                    self.runtime.instance_id.clone(),
                    self.runtime.kind,
                    selected_model.clone(),
                    status_code,
                    request_id,
                    vec![attempt_record],
                ));
                let terminal_error = terminal_failure_error(&error);
                self.emit_request_end_failure(
                    &context,
                    DirectFailureContext {
                        request_id: terminal_error.request_id.clone(),
                        provider: terminal_error.provider,
                        model: Some(selected_model),
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
        self.execute_stream(
            input.into_task_request()?,
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
    }

    pub(crate) async fn execute_stream(
        &self,
        task: TaskRequest,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        self.execute_stream_on_attempt(task, self.default_attempt(), execution)
            .await
    }

    pub(crate) async fn execute_stream_on_attempt(
        &self,
        task: TaskRequest,
        attempt: AttemptSpec,
        execution: ExecutionOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        if execution.response_mode != ResponseMode::Streaming {
            return Err(RuntimeError::configuration(
                "streaming() requires ExecutionOptions.response_mode = ResponseMode::Streaming",
            ));
        }
        let execution_plan = planner::plan_direct_attempt(self, &task, &attempt, &execution)?;
        let context = self.begin_direct_request(execution_plan.provider_attempt.model.as_str());
        let stream_observer = context.cloned_observer();

        match self.runtime.open_stream_attempt(execution_plan).await {
            ProviderStreamAttemptOutcome::Opened {
                stream,
                selected_model,
                status_code,
                request_id,
            } => Ok(MessageResponseStream::new_direct(
                context.request_started_at,
                stream_observer,
                LiveAttempt {
                    stream: *stream,
                    context: AttemptContext {
                        target_index: 0,
                        attempt_index: 0,
                        started_at: context.attempt_started_at,
                        observer: context.cloned_observer(),
                        provider_instance: self.runtime.instance_id.clone(),
                        provider: self.runtime.kind,
                        model: selected_model,
                        request_id,
                        status_code,
                    },
                },
            )),
            ProviderStreamAttemptOutcome::Failure {
                error,
                selected_model,
            } => {
                let attempt_record = failed_attempt_record(
                    self.runtime.instance_id.clone(),
                    self.runtime.kind,
                    selected_model.clone(),
                    0,
                    0,
                    &error,
                );
                self.emit_attempt_failure(&context, &attempt_record);
                let status_code = error.status_code;
                let request_id = error.request_id.clone();
                let error = error.with_executed_failure_meta(executed_failure_meta(
                    self.runtime.instance_id.clone(),
                    self.runtime.kind,
                    selected_model.clone(),
                    status_code,
                    request_id,
                    vec![attempt_record],
                ));
                self.emit_request_end_failure(
                    &context,
                    DirectFailureContext {
                        request_id: error.request_id.clone(),
                        provider: Some(self.runtime.kind),
                        model: Some(selected_model),
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

    fn emit_attempt_success(&self, context: &DirectRequestContext<'_>, attempt: &AttemptRecord) {
        let event = attempt_success_event(attempt, context.attempt_started_at.elapsed());
        safe_call_observer(context.observer, |runtime_observer| {
            runtime_observer.on_attempt_success(&event);
        });
    }

    fn emit_attempt_failure(&self, context: &DirectRequestContext<'_>, attempt: &AttemptRecord) {
        let event = attempt_failure_event(attempt, context.attempt_started_at.elapsed());
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
            provider: Some(response_meta.selected_provider_kind),
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

    pub(crate) fn default_attempt(&self) -> AttemptSpec {
        AttemptSpec::to(Target::new(self.runtime.instance_id.clone()))
    }
}

impl DirectRequestContext<'_> {
    fn cloned_observer(&self) -> Option<Arc<dyn RuntimeObserver>> {
        self.observer.cloned()
    }
}

fn request_model(model_id: &str) -> Option<String> {
    (!model_id.is_empty()).then(|| model_id.to_string())
}
