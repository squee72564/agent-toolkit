use std::sync::Mutex;

use agent_core::ProviderKind;
use agent_providers::adapter::adapter_for;
use agent_transport::HttpTransport;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::{
    AttemptFailureEvent, AttemptSkippedEvent, AttemptStartEvent, AttemptSuccessEvent,
    RequestEndEvent, RequestStartEvent, RuntimeObserver,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedEvent {
    RequestStart(RequestStartEvent),
    AttemptStart(AttemptStartEvent),
    AttemptSkipped(AttemptSkippedEvent),
    AttemptFailure(AttemptFailureEvent),
    AttemptSuccess(AttemptSuccessEvent),
    RequestEnd(RequestEndEvent),
}

impl RecordedEvent {
    pub fn name(&self) -> &'static str {
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
pub struct RecordingObserver {
    events: Mutex<Vec<RecordedEvent>>,
}

impl RecordingObserver {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    pub fn snapshot(&self) -> Vec<RecordedEvent> {
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

pub fn event_names(events: &[RecordedEvent]) -> Vec<&'static str> {
    events.iter().map(RecordedEvent::name).collect()
}

pub fn as_attempt_skipped(event: &RecordedEvent) -> &AttemptSkippedEvent {
    match event {
        RecordedEvent::AttemptSkipped(inner) => inner,
        other => panic!("expected attempt_skipped event, got {}", other.name()),
    }
}

pub fn test_streaming_provider_client(
    provider: ProviderKind,
    base_url: &str,
    default_model: Option<&str>,
) -> crate::provider_client::ProviderClient {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let instance_id = crate::test::default_instance_id(provider);
    let mut config = crate::ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = crate::RegisteredProvider::new(instance_id.clone(), provider, config);
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

    crate::provider_client::ProviderClient::new(crate::provider_runtime::ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    })
}

pub async fn spawn_sse_stub(content_type: &str, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let content_type = content_type.to_string();
    let body = body.to_string();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept test stream");
        let mut scratch = [0_u8; 8192];
        let _ = stream.read(&mut scratch).await;

        let http = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nx-request-id: req_sse\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(http.as_bytes()).await;
        let _ = stream.shutdown().await;
    });

    format!("http://{addr}")
}
