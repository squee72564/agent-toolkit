use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use agent_core::{CanonicalStreamEnvelope, Request, Response};
use futures_core::Stream;

use crate::message_text_stream::MessageTextStream;
use crate::observer::RuntimeObserver;
use crate::runtime_error::RuntimeError;
use crate::types::ResponseMeta;

mod driver;
mod events;
mod state;

use self::driver::{DriveNextOutcome, drive_next};
use self::state::StreamDriverState;
pub(crate) use self::state::{AttemptContext, LiveAttempt, RoutedStreamInit};

type InFlightFuture = Pin<Box<dyn Future<Output = (StreamDriverState, DriveNextOutcome)> + Send>>;

/// Completion payload returned after a streaming response has been fully finalized.
///
/// `MessageResponseStream` iteration yields only stream envelopes or terminal errors. Successful
/// completion metadata is retrieved explicitly by calling [`MessageResponseStream::finish`].
#[derive(Debug, Clone, PartialEq)]
pub struct StreamCompletion {
    pub response: Response,
    pub meta: ResponseMeta,
}

/// Stream of canonical response envelopes from a provider attempt.
///
/// Iteration yields only stream payload items and terminal errors. When iteration returns `None`,
/// the payload stream is exhausted, but successful completion metadata has not been returned yet.
/// Call [`MessageResponseStream::finish`] after draining the stream to retrieve the final
/// [`Response`] and [`ResponseMeta`].
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
            state: Some(StreamDriverState::new_direct(
                request,
                request_started_at,
                request_observer,
                attempt,
            )),
            in_flight: None,
        }
    }

    pub(crate) fn new_routed(init: RoutedStreamInit<'_>) -> Self {
        Self {
            state: Some(StreamDriverState::new_routed(init)),
            in_flight: None,
        }
    }

    /// Finalize the stream and return terminal success metadata.
    ///
    /// This method must be called to retrieve successful completion details, even if the stream
    /// has already been fully drained via iteration. If a terminal error was already surfaced
    /// during iteration, this returns that same error.
    pub async fn finish(mut self) -> Result<StreamCompletion, RuntimeError> {
        let mut state = self.take_state().await?;

        loop {
            if let Some(completion) = state.pending_completion.take() {
                events::emit_attempt_success(&state.request_observer, &completion.attempt);
                events::emit_request_end_success(&state, &completion.attempt);
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
