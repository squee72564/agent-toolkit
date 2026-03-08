use std::sync::Arc;

use crate::types::{
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent, RequestStartEvent,
};

pub trait RuntimeObserver: Send + Sync {
    fn on_request_start(&self, _event: &RequestStartEvent) {}
    fn on_attempt_start(&self, _event: &AttemptStartEvent) {}
    fn on_attempt_success(&self, _event: &AttemptSuccessEvent) {}
    fn on_attempt_failure(&self, _event: &AttemptFailureEvent) {}
    fn on_request_end(&self, _event: &RequestEndEvent) {}
}

pub fn resolve_observer_for_request<'a>(
    client_observer: Option<&'a Arc<dyn RuntimeObserver>>,
    toolkit_observer: Option<&'a Arc<dyn RuntimeObserver>>,
    send_observer: Option<&'a Arc<dyn RuntimeObserver>>,
) -> Option<&'a Arc<dyn RuntimeObserver>> {
    send_observer.or(toolkit_observer).or(client_observer)
}

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
