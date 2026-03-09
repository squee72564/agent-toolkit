use super::*;
use crate::observer::{resolve_observer_for_request, safe_call_observer};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug)]
struct PanicObserver;

impl RuntimeObserver for PanicObserver {
    fn on_request_start(&self, _event: &RequestStartEvent) {
        panic!("observer panic should be suppressed");
    }
}

#[derive(Debug)]
struct CountingObserver {
    call_count: AtomicUsize,
}

impl CountingObserver {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl RuntimeObserver for CountingObserver {
    fn on_request_start(&self, _event: &RequestStartEvent) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }
}

fn request_start_event() -> RequestStartEvent {
    RequestStartEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: Duration::ZERO,
        first_target: None,
        resolved_target_count: 1,
    }
}

#[test]
fn resolve_observer_for_request_uses_expected_precedence() {
    let client_observer: Arc<dyn RuntimeObserver> = Arc::new(ObserverStub);
    let toolkit_observer: Arc<dyn RuntimeObserver> = Arc::new(ObserverStub);
    let send_observer: Arc<dyn RuntimeObserver> = Arc::new(ObserverStub);

    let resolved_send = resolve_observer_for_request(
        Some(&client_observer),
        Some(&toolkit_observer),
        Some(&send_observer),
    )
    .expect("send observer should resolve");
    assert!(Arc::ptr_eq(resolved_send, &send_observer));

    let resolved_toolkit =
        resolve_observer_for_request(Some(&client_observer), Some(&toolkit_observer), None)
            .expect("toolkit observer should resolve");
    assert!(Arc::ptr_eq(resolved_toolkit, &toolkit_observer));

    let resolved_client = resolve_observer_for_request(Some(&client_observer), None, None)
        .expect("client observer should resolve");
    assert!(Arc::ptr_eq(resolved_client, &client_observer));

    let resolved_none = resolve_observer_for_request(None, None, None);
    assert!(resolved_none.is_none());
}

#[test]
fn safe_call_observer_suppresses_panics() {
    let observer: Arc<dyn RuntimeObserver> = Arc::new(PanicObserver);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        safe_call_observer(Some(&observer), |observer| {
            observer.on_request_start(&request_start_event());
        });
    }));

    assert!(result.is_ok(), "observer panic should not escape");
}

#[test]
fn safe_call_observer_with_none_is_a_noop() {
    let observer = CountingObserver::new();

    safe_call_observer(None, |runtime_observer| {
        runtime_observer.on_request_start(&request_start_event());
    });

    assert_eq!(observer.call_count(), 0);
}
