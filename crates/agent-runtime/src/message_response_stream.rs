use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use agent_core::{CanonicalStreamEnvelope, Request, Response};
use futures_core::Stream;

use crate::AgentToolkit;
use crate::message_text_stream::MessageTextStream;
use crate::observer::{RuntimeObserver, resolve_observer_for_request, safe_call_observer};
use crate::provider_runtime::{OpenedProviderStream, ProviderStreamAttemptOutcome};
use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::send_options::SendOptions;
use crate::target::Target;
use crate::types::{
    AttemptFailureEvent, AttemptMeta, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent,
    ResponseMeta,
};

type InFlightFuture = Pin<Box<dyn Future<Output = (StreamDriverState, DriveNextOutcome)> + Send>>;

#[derive(Debug, Clone, PartialEq)]
pub struct StreamCompletion {
    pub response: Response,
    pub meta: ResponseMeta,
}

pub struct MessageResponseStream {
    state: Option<StreamDriverState>,
    in_flight: Option<InFlightFuture>,
}

impl std::fmt::Debug for MessageResponseStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageResponseStream")
            .finish_non_exhaustive()
    }
}

impl MessageResponseStream {
    pub fn into_text_stream(self) -> MessageTextStream {
        MessageTextStream::new(self)
    }

    pub(crate) fn new_direct(
        request: Request,
        request_started_at: Instant,
        request_observer: Option<Arc<dyn RuntimeObserver>>,
        attempt: LiveAttempt,
    ) -> Self {
        Self {
            state: Some(StreamDriverState {
                request_started_at,
                request_observer,
                request_model_id: request.model_id,
                attempts: Vec::new(),
                current_attempt: Some(attempt),
                routing: RoutingState::Direct,
                first_envelope_emitted: false,
                pending_completion: None,
                terminal_error: None,
                terminal_error_delivered: false,
            }),
            in_flight: None,
        }
    }

    pub(crate) fn new_routed(init: RoutedStreamInit<'_>) -> Self {
        Self {
            state: Some(StreamDriverState {
                request_started_at: init.request_started_at,
                request_observer: init.request_observer,
                request_model_id: init.request.model_id.clone(),
                attempts: Vec::new(),
                current_attempt: Some(init.current_attempt),
                routing: RoutingState::Routed(Box::new(RoutedState {
                    toolkit: init.toolkit.clone(),
                    request: init.request,
                    options: init.options,
                    targets: init.targets,
                    next_target_index: init.next_target_index,
                })),
                first_envelope_emitted: false,
                pending_completion: None,
                terminal_error: None,
                terminal_error_delivered: false,
            }),
            in_flight: None,
        }
    }

    pub async fn finish(mut self) -> Result<StreamCompletion, RuntimeError> {
        let mut state = self.take_state().await?;

        loop {
            if let Some(completion) = state.pending_completion.take() {
                emit_attempt_success(&state.request_observer, &completion.attempt);
                emit_request_end_success(&state, &completion.attempt);
                let response = completion.response.clone();
                let meta = completion.meta(state.attempts);
                return Ok(StreamCompletion { response, meta });
            }

            if let Some(error) = state.terminal_error.take() {
                return Err(error);
            }

            let (next_state, outcome) = drive_next(state).await;
            state = next_state;
            match outcome {
                DriveNextOutcome::Envelope(_) | DriveNextOutcome::TerminalError(_) => {}
                DriveNextOutcome::Exhausted => {}
            }
        }
    }

    async fn take_state(&mut self) -> Result<StreamDriverState, RuntimeError> {
        if let Some(in_flight) = self.in_flight.take() {
            let (state, outcome) = in_flight.await;
            let mut state = state;
            if let DriveNextOutcome::TerminalError(error) = outcome {
                state.terminal_error = Some(error);
            }
            return Ok(state);
        }

        self.state.take().ok_or_else(|| {
            RuntimeError::configuration("message response stream state was already consumed")
        })
    }
}

impl Stream for MessageResponseStream {
    type Item = Result<CanonicalStreamEnvelope, RuntimeError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();

        if let Some(in_flight) = this.in_flight.as_mut() {
            match in_flight.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready((state, outcome)) => {
                    this.in_flight = None;
                    this.state = Some(state);
                    return match outcome {
                        DriveNextOutcome::Envelope(envelope) => Poll::Ready(Some(Ok(envelope))),
                        DriveNextOutcome::TerminalError(error) => {
                            if let Some(state) = this.state.as_mut() {
                                state.terminal_error = Some(error.clone());
                                state.terminal_error_delivered = true;
                            }
                            Poll::Ready(Some(Err(error)))
                        }
                        DriveNextOutcome::Exhausted => Poll::Ready(None),
                    };
                }
            }
        }

        let Some(mut state) = this.state.take() else {
            return Poll::Ready(None);
        };

        if let Some(error) = state.terminal_error.clone() {
            if state.terminal_error_delivered {
                this.state = Some(state);
                return Poll::Ready(None);
            }
            state.terminal_error_delivered = true;
            this.state = Some(state);
            return Poll::Ready(Some(Err(error)));
        }

        if state.pending_completion.is_some() {
            this.state = Some(state);
            return Poll::Ready(None);
        }

        let future = Box::pin(async move { drive_next(state).await });
        this.in_flight = Some(future);
        Pin::new(this).poll_next(cx)
    }
}

struct StreamDriverState {
    request_started_at: Instant,
    request_observer: Option<Arc<dyn RuntimeObserver>>,
    request_model_id: String,
    attempts: Vec<AttemptMeta>,
    current_attempt: Option<LiveAttempt>,
    routing: RoutingState,
    first_envelope_emitted: bool,
    pending_completion: Option<PendingCompletion>,
    terminal_error: Option<RuntimeError>,
    terminal_error_delivered: bool,
}

enum RoutingState {
    Direct,
    Routed(Box<RoutedState>),
}

struct RoutedState {
    toolkit: AgentToolkit,
    request: Request,
    options: SendOptions,
    targets: Vec<Target>,
    next_target_index: usize,
}

pub(crate) struct RoutedStreamInit<'a> {
    pub(crate) request: Request,
    pub(crate) toolkit: &'a AgentToolkit,
    pub(crate) options: SendOptions,
    pub(crate) request_started_at: Instant,
    pub(crate) request_observer: Option<Arc<dyn RuntimeObserver>>,
    pub(crate) targets: Vec<Target>,
    pub(crate) current_attempt: LiveAttempt,
    pub(crate) next_target_index: usize,
}

struct PendingCompletion {
    response: Response,
    attempt: CompletedAttemptContext,
    selected_provider: agent_core::ProviderId,
    selected_model: String,
    status_code: Option<u16>,
    request_id: Option<String>,
}

impl PendingCompletion {
    fn meta(self, mut attempts: Vec<AttemptMeta>) -> ResponseMeta {
        attempts.push(self.attempt.meta.clone());
        ResponseMeta {
            selected_provider: self.selected_provider,
            selected_model: self.selected_model,
            status_code: self.status_code,
            request_id: self.request_id,
            attempts,
        }
    }
}

pub(crate) struct LiveAttempt {
    pub(crate) stream: OpenedProviderStream,
    pub(crate) context: AttemptContext,
}

pub(crate) struct AttemptContext {
    pub(crate) target_index: usize,
    pub(crate) attempt_index: usize,
    pub(crate) started_at: Instant,
    pub(crate) observer: Option<Arc<dyn RuntimeObserver>>,
    pub(crate) provider: agent_core::ProviderId,
    pub(crate) model: String,
    pub(crate) request_id: Option<String>,
    pub(crate) status_code: Option<u16>,
}

struct CompletedAttemptContext {
    target_index: usize,
    attempt_index: usize,
    started_at: Instant,
    observer: Option<Arc<dyn RuntimeObserver>>,
    meta: AttemptMeta,
}

enum DriveNextOutcome {
    Envelope(CanonicalStreamEnvelope),
    TerminalError(RuntimeError),
    Exhausted,
}

async fn drive_next(mut state: StreamDriverState) -> (StreamDriverState, DriveNextOutcome) {
    loop {
        if let Some(error) = state.terminal_error.clone() {
            return (state, DriveNextOutcome::TerminalError(error));
        }

        let Some(mut attempt) = state.current_attempt.take() else {
            state.terminal_error = Some(RuntimeError::configuration(
                "stream driver reached an invalid state without an active attempt",
            ));
            continue;
        };

        match attempt.stream.next_envelope().await {
            Ok(Some(envelope)) => {
                state.first_envelope_emitted = true;
                state.current_attempt = Some(attempt);
                return (state, DriveNextOutcome::Envelope(envelope));
            }
            Ok(None) => match attempt.stream.finish() {
                Ok((response, http_response)) => {
                    let attempt_meta = AttemptMeta {
                        provider: attempt.context.provider,
                        model: attempt.context.model.clone(),
                        success: true,
                        status_code: Some(http_response.head.status.as_u16()),
                        request_id: http_response.head.request_id.clone(),
                        error_kind: None,
                        error_message: None,
                    };
                    state.pending_completion = Some(PendingCompletion {
                        response,
                        attempt: CompletedAttemptContext {
                            target_index: attempt.context.target_index,
                            attempt_index: attempt.context.attempt_index,
                            started_at: attempt.context.started_at,
                            observer: attempt.context.observer.clone(),
                            meta: attempt_meta,
                        },
                        selected_provider: attempt.context.provider,
                        selected_model: attempt.context.model.clone(),
                        status_code: Some(http_response.head.status.as_u16()),
                        request_id: http_response.head.request_id.clone(),
                    });
                    return (state, DriveNextOutcome::Exhausted);
                }
                Err(error) => {
                    if let Some(next_attempt) = try_open_fallback_attempt(&mut state, &error).await
                    {
                        emit_attempt_failure(
                            attempt.context.observer.as_ref(),
                            &attempt_failure_meta(&attempt.context, &error),
                            attempt.context.target_index,
                            attempt.context.attempt_index,
                            attempt.context.started_at,
                        );
                        state
                            .attempts
                            .push(attempt_failure_meta(&attempt.context, &error));
                        state.current_attempt = Some(next_attempt);
                        continue;
                    }
                    let failure_meta = attempt_failure_meta(&attempt.context, &error);
                    emit_attempt_failure(
                        attempt.context.observer.as_ref(),
                        &failure_meta,
                        attempt.context.target_index,
                        attempt.context.attempt_index,
                        attempt.context.started_at,
                    );
                    state.attempts.push(failure_meta);
                    emit_request_end_failure(&state, &attempt.context, &error);
                    state.terminal_error = Some(error.clone());
                    return (state, DriveNextOutcome::TerminalError(error));
                }
            },
            Err(error) => {
                if let Some(next_attempt) = try_open_fallback_attempt(&mut state, &error).await {
                    emit_attempt_failure(
                        attempt.context.observer.as_ref(),
                        &attempt_failure_meta(&attempt.context, &error),
                        attempt.context.target_index,
                        attempt.context.attempt_index,
                        attempt.context.started_at,
                    );
                    state
                        .attempts
                        .push(attempt_failure_meta(&attempt.context, &error));
                    state.current_attempt = Some(next_attempt);
                    continue;
                }
                let failure_meta = attempt_failure_meta(&attempt.context, &error);
                emit_attempt_failure(
                    attempt.context.observer.as_ref(),
                    &failure_meta,
                    attempt.context.target_index,
                    attempt.context.attempt_index,
                    attempt.context.started_at,
                );
                state.attempts.push(failure_meta);
                emit_request_end_failure(&state, &attempt.context, &error);
                state.terminal_error = Some(error.clone());
                return (state, DriveNextOutcome::TerminalError(error));
            }
        }
    }
}

async fn try_open_fallback_attempt(
    state: &mut StreamDriverState,
    error: &RuntimeError,
) -> Option<LiveAttempt> {
    if state.first_envelope_emitted {
        return None;
    }

    let RoutingState::Routed(routed) = &mut state.routing else {
        return None;
    };

    while routed.next_target_index < routed.targets.len() {
        if !routed
            .options
            .fallback_policy
            .as_ref()
            .is_some_and(|policy| policy.should_fallback(error))
        {
            return None;
        }

        let index = routed.next_target_index;
        routed.next_target_index = routed.next_target_index.saturating_add(1);
        let target = routed.targets[index].clone();
        let client = routed.toolkit.clients.get(&target.provider)?;
        let observer = resolve_observer_for_request(
            client.runtime.observer.as_ref(),
            routed.toolkit.observer.as_ref(),
            routed.options.observer.as_ref(),
        )
        .cloned();
        let attempt_started_at = Instant::now();
        emit_attempt_start(
            observer.as_ref(),
            target.provider,
            event_model(target.model.as_deref(), &state.request_model_id),
            index,
            index,
            attempt_started_at,
        );
        let request = routed.request.clone();
        match client
            .runtime
            .open_stream_attempt(
                request,
                target.model.as_deref(),
                routed.options.metadata.clone(),
            )
            .await
        {
            ProviderStreamAttemptOutcome::Opened { stream, meta } => {
                return Some(LiveAttempt {
                    stream: *stream,
                    context: AttemptContext {
                        target_index: index,
                        attempt_index: index,
                        started_at: attempt_started_at,
                        observer,
                        provider: meta.provider,
                        model: meta.model,
                        request_id: meta.request_id,
                        status_code: meta.status_code,
                    },
                });
            }
            ProviderStreamAttemptOutcome::Failure {
                error: attempt_error,
                meta,
            } => {
                emit_attempt_failure(observer.as_ref(), &meta, index, index, attempt_started_at);
                state.attempts.push(meta);
                if routed.next_target_index >= routed.targets.len()
                    || !routed
                        .options
                        .fallback_policy
                        .as_ref()
                        .is_some_and(|policy| policy.should_fallback(&attempt_error))
                {
                    emit_request_end_failure(
                        state,
                        &AttemptContext {
                            target_index: index,
                            attempt_index: index,
                            started_at: attempt_started_at,
                            observer,
                            provider: target.provider,
                            model: target.model.unwrap_or_default(),
                            request_id: attempt_error.request_id.clone(),
                            status_code: attempt_error.status_code,
                        },
                        &attempt_error,
                    );
                    state.terminal_error = Some(attempt_error);
                    return None;
                }
            }
        }
    }

    None
}

fn attempt_failure_meta(context: &AttemptContext, error: &RuntimeError) -> AttemptMeta {
    AttemptMeta {
        provider: context.provider,
        model: context.model.clone(),
        success: false,
        status_code: error.status_code.or(context.status_code),
        request_id: error
            .request_id
            .clone()
            .or_else(|| context.request_id.clone()),
        error_kind: Some(error.kind),
        error_message: Some(error.message.clone()),
    }
}

fn emit_attempt_start(
    observer: Option<&Arc<dyn RuntimeObserver>>,
    provider: agent_core::ProviderId,
    model: Option<String>,
    target_index: usize,
    attempt_index: usize,
    started_at: Instant,
) {
    let event = AttemptStartEvent {
        request_id: None,
        provider: Some(provider),
        model,
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed: started_at.elapsed(),
    };
    safe_call_observer(observer, |runtime_observer| {
        runtime_observer.on_attempt_start(&event);
    });
}

fn emit_attempt_success(
    request_observer: &Option<Arc<dyn RuntimeObserver>>,
    attempt: &CompletedAttemptContext,
) {
    let event = AttemptSuccessEvent {
        request_id: attempt.meta.request_id.clone(),
        provider: Some(attempt.meta.provider),
        model: Some(attempt.meta.model.clone()),
        target_index: Some(attempt.target_index),
        attempt_index: Some(attempt.attempt_index),
        elapsed: attempt.started_at.elapsed(),
        status_code: attempt.meta.status_code,
    };
    safe_call_observer(
        attempt.observer.as_ref().or(request_observer.as_ref()),
        |observer| {
            observer.on_attempt_success(&event);
        },
    );
}

fn emit_attempt_failure(
    observer: Option<&Arc<dyn RuntimeObserver>>,
    meta: &AttemptMeta,
    target_index: usize,
    attempt_index: usize,
    started_at: Instant,
) {
    let event = AttemptFailureEvent {
        request_id: meta.request_id.clone(),
        provider: Some(meta.provider),
        model: Some(meta.model.clone()),
        target_index: Some(target_index),
        attempt_index: Some(attempt_index),
        elapsed: started_at.elapsed(),
        error_kind: meta.error_kind,
        error_message: meta.error_message.clone(),
    };
    safe_call_observer(observer, |runtime_observer| {
        runtime_observer.on_attempt_failure(&event);
    });
}

fn emit_request_end_success(state: &StreamDriverState, attempt: &CompletedAttemptContext) {
    let event = RequestEndEvent {
        request_id: attempt.meta.request_id.clone(),
        provider: Some(attempt.meta.provider),
        model: Some(attempt.meta.model.clone()),
        target_index: Some(attempt.target_index),
        attempt_index: Some(attempt.attempt_index),
        elapsed: state.request_started_at.elapsed(),
        status_code: attempt.meta.status_code,
        error_kind: None,
        error_message: None,
    };
    safe_call_observer(state.request_observer.as_ref(), |observer| {
        observer.on_request_end(&event);
    });
}

fn emit_request_end_failure(
    state: &StreamDriverState,
    attempt: &AttemptContext,
    error: &RuntimeError,
) {
    let terminal_error = terminal_failure_error(error);
    let event = RequestEndEvent {
        request_id: terminal_error
            .request_id
            .clone()
            .or_else(|| attempt.request_id.clone()),
        provider: terminal_error.provider.or(Some(attempt.provider)),
        model: Some(attempt.model.clone()),
        target_index: Some(attempt.target_index),
        attempt_index: Some(attempt.attempt_index),
        elapsed: state.request_started_at.elapsed(),
        status_code: terminal_error.status_code.or(attempt.status_code),
        error_kind: Some(terminal_error.kind),
        error_message: Some(terminal_error.message.clone()),
    };
    safe_call_observer(state.request_observer.as_ref(), |observer| {
        observer.on_request_end(&event);
    });
}

fn event_model(target_model: Option<&str>, request_model: &str) -> Option<String> {
    trimmed_non_empty(target_model.unwrap_or_default())
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
