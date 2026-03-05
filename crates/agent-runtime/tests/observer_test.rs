use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use agent_core::types::ProviderId;
use agent_runtime::{
    AgentToolkit, AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, FallbackMode,
    FallbackPolicy, FallbackRule, MessageCreateInput, ProviderConfig, RequestEndEvent,
    RequestStartEvent, RuntimeErrorKind, RuntimeObserver, SendOptions, Target, openai,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const OPENAI_SUCCESS_BODY: &str = include_str!(
    "../../agent-providers/data/openai/responses/2026-02-27T03:25:13.281Z/basic_chat/gpt-5-mini.json"
);

#[derive(Debug, Clone, PartialEq, Eq)]
enum RecordedEvent {
    RequestStart(RequestStartEvent),
    AttemptStart(AttemptStartEvent),
    AttemptSuccess(AttemptSuccessEvent),
    AttemptFailure(AttemptFailureEvent),
    RequestEnd(RequestEndEvent),
}

impl RecordedEvent {
    fn name(&self) -> &'static str {
        match self {
            Self::RequestStart(_) => "request_start",
            Self::AttemptStart(_) => "attempt_start",
            Self::AttemptSuccess(_) => "attempt_success",
            Self::AttemptFailure(_) => "attempt_failure",
            Self::RequestEnd(_) => "request_end",
        }
    }
}

#[derive(Debug)]
struct RecordingObserver {
    events: Mutex<Vec<RecordedEvent>>,
    panic_on: Option<&'static str>,
    panicked_once: AtomicBool,
}

impl RecordingObserver {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            panic_on: None,
            panicked_once: AtomicBool::new(false),
        }
    }

    fn with_panic(panic_on: &'static str) -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            panic_on: Some(panic_on),
            panicked_once: AtomicBool::new(false),
        }
    }

    fn snapshot(&self) -> Vec<RecordedEvent> {
        self.events
            .lock()
            .expect("observer event mutex poisoned")
            .clone()
    }

    fn maybe_panic(&self, event_name: &'static str) {
        if self.panic_on == Some(event_name) && !self.panicked_once.swap(true, Ordering::SeqCst) {
            panic!("observer panic on {event_name}");
        }
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
        self.maybe_panic("request_start");
    }

    fn on_attempt_start(&self, event: &AttemptStartEvent) {
        self.record(RecordedEvent::AttemptStart(event.clone()));
        self.maybe_panic("attempt_start");
    }

    fn on_attempt_success(&self, event: &AttemptSuccessEvent) {
        self.record(RecordedEvent::AttemptSuccess(event.clone()));
        self.maybe_panic("attempt_success");
    }

    fn on_attempt_failure(&self, event: &AttemptFailureEvent) {
        self.record(RecordedEvent::AttemptFailure(event.clone()));
        self.maybe_panic("attempt_failure");
    }

    fn on_request_end(&self, event: &RequestEndEvent) {
        self.record(RecordedEvent::RequestEnd(event.clone()));
        self.maybe_panic("request_end");
    }
}

#[derive(Debug, Clone)]
struct StubHttpResponse {
    status: u16,
    request_id: String,
    body: String,
}

impl StubHttpResponse {
    fn success(request_id: &str) -> Self {
        Self {
            status: 200,
            request_id: request_id.to_string(),
            body: OPENAI_SUCCESS_BODY.to_string(),
        }
    }
}

async fn spawn_openai_stub(responses: Vec<StubHttpResponse>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let response_queue = Arc::new(tokio::sync::Mutex::new(VecDeque::from(responses)));

    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };

            let response = {
                let mut queue = response_queue.lock().await;
                queue.pop_front().unwrap_or_else(|| StubHttpResponse {
                    status: 500,
                    request_id: "stub_queue_empty".to_string(),
                    body: "{\"error\":{\"message\":\"stub queue empty\"}}".to_string(),
                })
            };

            tokio::spawn(async move {
                let mut scratch = [0_u8; 8192];
                let _ = stream.read(&mut scratch).await;

                let reason = if response.status == 200 {
                    "OK"
                } else {
                    "ERROR"
                };
                let http = format!(
                    "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-request-id: {}\r\nconnection: close\r\n\r\n{}",
                    response.status,
                    reason,
                    response.body.len(),
                    response.request_id,
                    response.body
                );
                let _ = stream.write_all(http.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    format!("http://{addr}")
}

fn unused_local_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("ephemeral local addr");
    drop(listener);
    format!("http://{addr}")
}

fn event_names(events: &[RecordedEvent]) -> Vec<&'static str> {
    events.iter().map(RecordedEvent::name).collect()
}

#[tokio::test]
async fn observer_callbacks_direct_lifecycle_success() {
    let base_url = spawn_openai_stub(vec![StubHttpResponse::success("req_success")]).await;
    let observer = Arc::new(RecordingObserver::new());

    let client = openai()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("gpt-5-mini")
        .observer(observer.clone())
        .build()
        .expect("build direct client");

    let (_response, meta) = client
        .messages()
        .create_with_meta(MessageCreateInput::user("hello"))
        .await
        .expect("direct request should succeed");

    assert_eq!(meta.attempts.len(), 1);

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec![
            "request_start",
            "attempt_start",
            "attempt_success",
            "request_end"
        ]
    );

    let attempt_elapsed = match &events[2] {
        RecordedEvent::AttemptSuccess(event) => event.elapsed,
        _ => panic!("expected attempt success event"),
    };
    let request_end = match &events[3] {
        RecordedEvent::RequestEnd(event) => event,
        _ => panic!("expected request end event"),
    };

    assert!(request_end.error_kind.is_none());
    assert!(request_end.request_id.is_some());
    assert!(request_end.elapsed >= attempt_elapsed);
}

#[tokio::test]
async fn observer_callbacks_direct_lifecycle_failure() {
    let observer = Arc::new(RecordingObserver::new());

    let client = openai()
        .api_key("test-key")
        .base_url(unused_local_url())
        .default_model("gpt-5-mini")
        .observer(observer.clone())
        .build()
        .expect("build direct client");

    let error = client
        .messages()
        .create_with_meta(MessageCreateInput::user("hello"))
        .await
        .expect_err("direct request should fail");

    assert_eq!(error.kind, RuntimeErrorKind::Transport);

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec![
            "request_start",
            "attempt_start",
            "attempt_failure",
            "request_end"
        ]
    );
    match &events[3] {
        RecordedEvent::RequestEnd(event) => {
            assert!(event.error_kind.is_some());
            assert!(event.error_message.is_some());
        }
        _ => panic!("expected request end event"),
    }
}

#[tokio::test]
async fn router_fallback_ordered_attempts_with_indices() {
    let base_url = spawn_openai_stub(vec![StubHttpResponse::success("req_router_success")]).await;
    let observer = Arc::new(RecordingObserver::new());

    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url(base_url))
        .observer(observer.clone())
        .build()
        .expect("build toolkit");

    let fallback_policy = FallbackPolicy::new(vec![
        Target::new(ProviderId::OpenAi).with_model("gpt-5-mini"),
    ])
    .with_mode(FallbackMode::RulesOnly)
    .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Configuration));

    let (_response, meta) = toolkit
        .messages()
        .create_with_meta(
            MessageCreateInput::user("hello"),
            SendOptions::for_target(Target::new(ProviderId::OpenAi).with_model(" "))
                .with_fallback_policy(fallback_policy),
        )
        .await
        .expect("router request should succeed on second attempt");

    assert_eq!(meta.attempts.len(), 2);

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec![
            "request_start",
            "attempt_start",
            "attempt_failure",
            "attempt_start",
            "attempt_success",
            "request_end"
        ]
    );

    match &events[1] {
        RecordedEvent::AttemptStart(event) => {
            assert_eq!(event.target_index, Some(0));
            assert_eq!(event.attempt_index, Some(0));
        }
        _ => panic!("expected first attempt_start"),
    }
    match &events[3] {
        RecordedEvent::AttemptStart(event) => {
            assert_eq!(event.target_index, Some(1));
            assert_eq!(event.attempt_index, Some(1));
        }
        _ => panic!("expected second attempt_start"),
    }
}

#[tokio::test]
async fn toolkit_observer_and_send_override_precedence() {
    let toolkit_observer = Arc::new(RecordingObserver::new());
    let send_observer = Arc::new(RecordingObserver::new());

    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .observer(toolkit_observer.clone())
        .build()
        .expect("build toolkit");

    let _ = toolkit
        .messages()
        .create_with_meta(
            MessageCreateInput::user("hello"),
            SendOptions::for_target(Target::new(ProviderId::OpenAi))
                .with_observer(send_observer.clone()),
        )
        .await
        .expect_err("request should fail and still emit observer events");

    assert!(toolkit_observer.snapshot().is_empty());
    assert_eq!(
        event_names(&send_observer.snapshot()),
        vec![
            "request_start",
            "attempt_start",
            "attempt_failure",
            "request_end"
        ]
    );
}

#[tokio::test]
async fn fallback_exhausted_request_end_uses_terminal_failure_context() {
    let observer = Arc::new(RecordingObserver::new());

    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .observer(observer.clone())
        .build()
        .expect("build toolkit");

    let fallback_policy =
        FallbackPolicy::new(vec![Target::new(ProviderId::OpenAi).with_model("  ")])
            .with_mode(FallbackMode::RulesOnly)
            .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Configuration));

    let error = toolkit
        .messages()
        .create_with_meta(
            MessageCreateInput::user("hello"),
            SendOptions::for_target(Target::new(ProviderId::OpenAi).with_model(" "))
                .with_fallback_policy(fallback_policy),
        )
        .await
        .expect_err("request should exhaust fallback");

    assert_eq!(error.kind, RuntimeErrorKind::FallbackExhausted);

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec![
            "request_start",
            "attempt_start",
            "attempt_failure",
            "attempt_start",
            "attempt_failure",
            "request_end"
        ]
    );

    match &events[5] {
        RecordedEvent::RequestEnd(event) => {
            assert_eq!(event.error_kind, Some(RuntimeErrorKind::Configuration));
            assert_ne!(event.error_kind, Some(RuntimeErrorKind::FallbackExhausted));
        }
        _ => panic!("expected request_end"),
    }
}

#[tokio::test]
async fn observer_panic_does_not_break_request_and_subsequent_callbacks() {
    let base_url = spawn_openai_stub(vec![StubHttpResponse::success("req_panic_safe")]).await;
    let observer = Arc::new(RecordingObserver::with_panic("attempt_start"));

    let client = openai()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("gpt-5-mini")
        .observer(observer.clone())
        .build()
        .expect("build direct client");

    let _ = client
        .messages()
        .create_with_meta(MessageCreateInput::user("hello"))
        .await
        .expect("request should still succeed despite observer panic");

    let events = observer.snapshot();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, RecordedEvent::AttemptSuccess(_)))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, RecordedEvent::RequestEnd(_)))
    );
}
