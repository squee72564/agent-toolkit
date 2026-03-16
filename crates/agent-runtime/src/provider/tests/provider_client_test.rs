use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use agent_core::{ProviderInstanceId, ProviderKind};

use crate::clients::{OpenAiClient, openai};
use crate::message::MessageCreateInput;
use crate::observability::RuntimeObserver;
use crate::routing::{AttemptSpec, Target};
use crate::test::observer_fixtures::{
    RecordingObserver, as_attempt_start, as_request_end, as_request_start, event_names,
};

use crate::{ExecutionOptions, RuntimeErrorKind};

const OPENAI_SUCCESS_BODY: &str = include_str!(
    "../../../../agent-providers/data/openai/responses/decoded/basic_chat/gpt-5-mini.json"
);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct StubHttpResponse {
    status: u16,
    request_id: String,
    content_type: String,
    body: String,
}

impl StubHttpResponse {
    fn json_success(request_id: &str) -> Self {
        Self {
            status: 200,
            request_id: request_id.to_string(),
            content_type: "application/json".to_string(),
            body: OPENAI_SUCCESS_BODY.to_string(),
        }
    }

    fn sse_success(request_id: &str, body: &str) -> Self {
        Self {
            status: 200,
            request_id: request_id.to_string(),
            content_type: "text/event-stream".to_string(),
            body: body.to_string(),
        }
    }
}

async fn with_timeout<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(REQUEST_TIMEOUT, future)
        .await
        .expect("request timed out in test")
}

async fn spawn_stub(response: StubHttpResponse) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept test stream");
        let mut scratch = [0_u8; 8192];
        let _ = stream.read(&mut scratch).await;

        let reason = if response.status == 200 {
            "OK"
        } else {
            "ERROR"
        };
        let http = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nx-request-id: {}\r\nconnection: close\r\n\r\n{}",
            response.status,
            reason,
            response.content_type,
            response.body.len(),
            response.request_id,
            response.body
        );
        let _ = stream.write_all(http.as_bytes()).await;
        let _ = stream.shutdown().await;
    });

    format!("http://{addr}")
}

fn unused_local_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("ephemeral local addr");
    drop(listener);
    format!("http://{addr}")
}

fn direct_client(base_url: String, observer: Arc<dyn RuntimeObserver>) -> OpenAiClient {
    openai()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("gpt-5-mini")
        .observer(observer)
        .build()
        .expect("build direct client")
}

#[tokio::test]
async fn direct_provider_client_non_stream_success_emits_expected_events() {
    let base_url = spawn_stub(StubHttpResponse::json_success("req_direct_success")).await;
    let observer = Arc::new(RecordingObserver::new());
    let client = direct_client(base_url, observer.clone());

    let (_response, meta) = with_timeout(
        client
            .messages()
            .create_with_meta(MessageCreateInput::user("hello")),
    )
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

    let request_start = as_request_start(&events[0]);
    let attempt_start = as_attempt_start(&events[1]);
    assert_eq!(request_start.provider, Some(ProviderKind::OpenAi));
    assert_eq!(request_start.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(attempt_start.provider, Some(ProviderKind::OpenAi));
    assert_eq!(attempt_start.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(attempt_start.target_index, Some(0));
    assert_eq!(attempt_start.attempt_index, Some(0));
}

#[tokio::test]
async fn direct_provider_client_explicit_task_api_uses_execution_boundary() {
    let base_url = spawn_stub(StubHttpResponse::json_success("req_direct_task")).await;
    let observer = Arc::new(RecordingObserver::new());
    let client = direct_client(base_url, observer.clone());

    let task = MessageCreateInput::user("hello explicit task")
        .into_task_request()
        .expect("task request should build");

    let (_response, meta) = with_timeout(
        client
            .messages()
            .execute_with_meta(task, ExecutionOptions::default()),
    )
    .await
    .expect("direct explicit task request should succeed");

    assert_eq!(
        meta.selected_provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(
        meta.selected_provider_kind,
        agent_core::ProviderKind::OpenAi
    );
    assert_eq!(meta.selected_model, "gpt-5-mini");
    assert_eq!(meta.attempts.len(), 1);
}

#[tokio::test]
async fn direct_provider_client_explicit_attempt_api_uses_attempt_model_override() {
    let base_url = spawn_stub(StubHttpResponse::json_success("req_direct_attempt")).await;
    let observer = Arc::new(RecordingObserver::new());
    let client = direct_client(base_url, observer);

    let task = MessageCreateInput::user("hello explicit attempt")
        .into_task_request()
        .expect("task request should build");

    let (_response, meta) = with_timeout(client.messages().execute_on_attempt_with_meta(
        task,
        AttemptSpec::to(Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5-mini")),
        ExecutionOptions::default(),
    ))
    .await
    .expect("direct explicit attempt request should succeed");

    assert_eq!(
        meta.selected_provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(
        meta.selected_provider_kind,
        agent_core::ProviderKind::OpenAi
    );
    assert_eq!(meta.selected_model, "gpt-5-mini");
    assert_eq!(meta.attempts.len(), 1);
}

#[tokio::test]
async fn direct_provider_client_non_stream_failure_emits_expected_events() {
    let observer = Arc::new(RecordingObserver::new());
    let client = direct_client(unused_local_url(), observer.clone());

    let error = with_timeout(
        client
            .messages()
            .create_with_meta(MessageCreateInput::user("hello")),
    )
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

    let request_end = as_request_end(&events[3]);
    assert_eq!(request_end.provider, Some(ProviderKind::OpenAi));
    assert_eq!(request_end.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(request_end.error_kind, Some(RuntimeErrorKind::Transport));
    assert!(request_end.error_message.is_some());
}

#[tokio::test]
async fn direct_provider_client_stream_open_success_only_emits_start_events() {
    let base_url = spawn_stub(StubHttpResponse::sse_success(
        "req_stream_open_success",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n"
        ),
    ))
    .await;
    let observer = Arc::new(RecordingObserver::new());
    let client = direct_client(base_url, observer.clone());

    let _stream = with_timeout(client.streaming().create(MessageCreateInput::user("hello")))
        .await
        .expect("stream should open");

    let events = observer.snapshot();
    assert_eq!(event_names(&events), vec!["request_start", "attempt_start"]);

    let request_start = as_request_start(&events[0]);
    let attempt_start = as_attempt_start(&events[1]);
    assert_eq!(request_start.provider, Some(ProviderKind::OpenAi));
    assert_eq!(request_start.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(attempt_start.provider, Some(ProviderKind::OpenAi));
    assert_eq!(attempt_start.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(attempt_start.target_index, Some(0));
    assert_eq!(attempt_start.attempt_index, Some(0));
}

#[tokio::test]
async fn direct_provider_client_stream_open_failure_emits_expected_events() {
    let observer = Arc::new(RecordingObserver::new());
    let client = direct_client(unused_local_url(), observer.clone());

    let error = with_timeout(client.streaming().create(MessageCreateInput::user("hello")))
        .await
        .expect_err("stream should fail to open");

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

    let request_end = as_request_end(&events[3]);
    assert_eq!(request_end.provider, Some(ProviderKind::OpenAi));
    assert_eq!(request_end.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(request_end.error_kind, Some(RuntimeErrorKind::Transport));
    assert!(request_end.error_message.is_some());
}
