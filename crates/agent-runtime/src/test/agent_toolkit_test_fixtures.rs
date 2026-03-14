use std::sync::Mutex;

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RecordedEvent {
    RequestStart(RequestStartEvent),
    AttemptStart(AttemptStartEvent),
    AttemptSkipped(AttemptSkippedEvent),
    AttemptFailure(AttemptFailureEvent),
    AttemptSuccess(AttemptSuccessEvent),
    RequestEnd(RequestEndEvent),
}

impl RecordedEvent {
    pub(super) fn name(&self) -> &'static str {
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
pub(super) struct RecordingObserver {
    events: Mutex<Vec<RecordedEvent>>,
}

impl RecordingObserver {
    pub(super) fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn snapshot(&self) -> Vec<RecordedEvent> {
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

pub(super) fn event_names(events: &[RecordedEvent]) -> Vec<&'static str> {
    events.iter().map(RecordedEvent::name).collect()
}

pub(super) fn as_attempt_skipped(event: &RecordedEvent) -> &AttemptSkippedEvent {
    match event {
        RecordedEvent::AttemptSkipped(inner) => inner,
        other => panic!("expected attempt_skipped event, got {}", other.name()),
    }
}

pub(super) async fn spawn_json_success_stub(request_id: &str) -> String {
    const OPENAI_SUCCESS_BODY: &str = include_str!(
        "../../../agent-providers/data/openai/responses/decoded/basic_chat/gpt-5-mini.json"
    );

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let request_id = request_id.to_string();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept test stream");
        let mut scratch = [0_u8; 8192];
        let _ = stream.read(&mut scratch).await;

        let http = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-request-id: {}\r\nconnection: close\r\n\r\n{}",
            OPENAI_SUCCESS_BODY.len(),
            request_id,
            OPENAI_SUCCESS_BODY
        );
        let _ = stream.write_all(http.as_bytes()).await;
        let _ = stream.shutdown().await;
    });

    format!("http://{addr}")
}
