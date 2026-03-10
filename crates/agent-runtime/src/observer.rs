use std::sync::Arc;

use crate::types::{
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent, RequestStartEvent,
};

/// Best-effort runtime lifecycle observer.
///
/// Observer callbacks are advisory only. Runtime code intentionally suppresses
/// observer panics so instrumentation cannot change request control flow or
/// fail an otherwise successful operation.
pub trait RuntimeObserver: Send + Sync {
    /// Called once when a request begins.
    fn on_request_start(&self, _event: &RequestStartEvent) {}
    /// Called when an individual provider attempt begins.
    fn on_attempt_start(&self, _event: &AttemptStartEvent) {}
    /// Called when an individual provider attempt succeeds.
    fn on_attempt_success(&self, _event: &AttemptSuccessEvent) {}
    /// Called when an individual provider attempt fails.
    fn on_attempt_failure(&self, _event: &AttemptFailureEvent) {}
    /// Called once when the overall request completes.
    fn on_request_end(&self, _event: &RequestEndEvent) {}
}

/// Resolves the observer to use for a request.
///
/// Precedence is explicit and stable: send-level observer overrides toolkit-
/// level observer, which overrides client-level observer.
pub fn resolve_observer_for_request<'a>(
    client_observer: Option<&'a Arc<dyn RuntimeObserver>>,
    toolkit_observer: Option<&'a Arc<dyn RuntimeObserver>>,
    send_observer: Option<&'a Arc<dyn RuntimeObserver>>,
) -> Option<&'a Arc<dyn RuntimeObserver>> {
    send_observer.or(toolkit_observer).or(client_observer)
}

/// Invokes an observer callback while suppressing observer panics.
///
/// This is intentional: observer failures must not alter runtime behavior, and
/// the runtime does not propagate observer panics back to callers.
pub fn safe_call_observer(
    observer: Option<&Arc<dyn RuntimeObserver>>,
    call: impl FnOnce(&dyn RuntimeObserver),
) {
    if let Some(observer) = observer {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            call(observer.as_ref());
        }));
    }
}
