use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use agent_core::{CanonicalStreamEnvelope, CanonicalStreamEvent};
use futures_core::Stream;
use futures_util::future::poll_fn;

use crate::{MessageResponseStream, RuntimeError, StreamCompletion};

/// Stream of assistant text deltas extracted from [`MessageResponseStream`].
///
/// Iteration yields only text chunks and terminal errors. When iteration returns `None`, callers
/// must still call [`MessageTextStream::finish`] to retrieve the final response and completion
/// metadata. `finish()` drains any buffered text state before finalizing the underlying response
/// stream so partial consumption cannot drop already-extracted text.
pub struct MessageTextStream {
    inner: MessageResponseStream,
    pending_text: VecDeque<String>,
}

impl std::fmt::Debug for MessageTextStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageTextStream").finish_non_exhaustive()
    }
}

impl MessageTextStream {
    pub(crate) fn new(inner: MessageResponseStream) -> Self {
        Self {
            inner,
            pending_text: VecDeque::new(),
        }
    }

    /// Finalize the text stream and return terminal success metadata.
    ///
    /// This method must be called to retrieve successful completion details, even after the text
    /// stream has been fully drained. If a terminal error was already surfaced during iteration,
    /// this returns that same error.
    pub async fn finish(mut self) -> Result<StreamCompletion, RuntimeError> {
        while poll_fn(|cx| Pin::new(&mut self).poll_next(cx))
            .await
            .transpose()?
            .is_some()
        {}

        self.inner.finish().await
    }

    pub(crate) fn enqueue_text_deltas(
        pending_text: &mut VecDeque<String>,
        envelope: CanonicalStreamEnvelope,
    ) {
        pending_text.extend(
            envelope
                .canonical
                .into_iter()
                .filter_map(|event| match event {
                    CanonicalStreamEvent::TextDelta { delta, .. } => Some(delta),
                    _ => None,
                }),
        );
    }
}

impl Stream for MessageTextStream {
    type Item = Result<String, RuntimeError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(delta) = self.pending_text.pop_front() {
                return Poll::Ready(Some(Ok(delta)));
            }

            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(envelope))) => {
                    Self::enqueue_text_deltas(&mut self.pending_text, envelope);
                }
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(None) => return Poll::Ready(None),
            }
        }
    }
}
