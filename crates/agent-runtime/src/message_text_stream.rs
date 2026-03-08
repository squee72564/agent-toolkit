use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use agent_core::{CanonicalStreamEnvelope, CanonicalStreamEvent};
use futures_core::Stream;

use crate::{MessageResponseStream, RuntimeError, StreamCompletion};

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

    pub async fn finish(self) -> Result<StreamCompletion, RuntimeError> {
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
