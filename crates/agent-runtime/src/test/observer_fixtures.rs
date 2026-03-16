use std::sync::Mutex;

use crate::observability::{
    AttemptFailureEvent, AttemptSkippedEvent, AttemptStartEvent, AttemptSuccessEvent,
    RequestEndEvent, RequestStartEvent, RuntimeObserver,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecordedEvent {
    RequestStart(RequestStartEvent),
    AttemptStart(AttemptStartEvent),
    AttemptSkipped(AttemptSkippedEvent),
    AttemptFailure(AttemptFailureEvent),
    AttemptSuccess(AttemptSuccessEvent),
    RequestEnd(RequestEndEvent),
}

impl RecordedEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::RequestStart(_) => "request_start",
            Self::AttemptStart(_) => "attempt_start",
            Self::AttemptSkipped(_) => "attempt_skipped",
            Self::AttemptFailure(_) => "attempt_failure",
            Self::AttemptSuccess(_) => "attempt_success",
            Self::RequestEnd(_) => "request_end",
        }
    }
}

#[derive(Debug)]
pub(crate) struct RecordingObserver {
    events: Mutex<Vec<RecordedEvent>>,
}

impl RecordingObserver {
    pub(crate) fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<RecordedEvent> {
        self.events
            .lock()
            .expect("observer event mutex poisoned")
            .clone()
    }

    fn record(&self, event: RecordedEvent) {
        self.events
            .lock()
            .expect("observer event mutex poisoned")
            .push(event);
    }
}

impl RuntimeObserver for RecordingObserver {
    fn on_request_start(&self, event: &RequestStartEvent) {
        self.record(RecordedEvent::RequestStart(event.clone()));
    }

    fn on_attempt_start(&self, event: &AttemptStartEvent) {
        self.record(RecordedEvent::AttemptStart(event.clone()));
    }

    fn on_attempt_skipped(&self, event: &AttemptSkippedEvent) {
        self.record(RecordedEvent::AttemptSkipped(event.clone()));
    }

    fn on_attempt_failure(&self, event: &AttemptFailureEvent) {
        self.record(RecordedEvent::AttemptFailure(event.clone()));
    }

    fn on_attempt_success(&self, event: &AttemptSuccessEvent) {
        self.record(RecordedEvent::AttemptSuccess(event.clone()));
    }

    fn on_request_end(&self, event: &RequestEndEvent) {
        self.record(RecordedEvent::RequestEnd(event.clone()));
    }
}

pub(crate) fn event_names(events: &[RecordedEvent]) -> Vec<&'static str> {
    events.iter().map(RecordedEvent::name).collect()
}

pub(crate) fn as_attempt_skipped(event: &RecordedEvent) -> &AttemptSkippedEvent {
    match event {
        RecordedEvent::AttemptSkipped(inner) => inner,
        other => panic!("expected attempt_skipped event, got {}", other.name()),
    }
}

pub(crate) fn as_request_start(event: &RecordedEvent) -> &RequestStartEvent {
    match event {
        RecordedEvent::RequestStart(inner) => inner,
        other => panic!("expected request_start event, got {}", other.name()),
    }
}

pub(crate) fn as_attempt_start(event: &RecordedEvent) -> &AttemptStartEvent {
    match event {
        RecordedEvent::AttemptStart(inner) => inner,
        other => panic!("expected attempt_start event, got {}", other.name()),
    }
}

pub(crate) fn as_request_end(event: &RecordedEvent) -> &RequestEndEvent {
    match event {
        RecordedEvent::RequestEnd(inner) => inner,
        other => panic!("expected request_end event, got {}", other.name()),
    }
}
