use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agent_core::{CanonicalStreamEnvelope, CanonicalStreamEvent, ProviderKind};
use agent_providers::adapter::adapter_for;
use agent_transport::HttpTransport;
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::{AgentToolkit, ExecutionOptions, MessageCreateInput, Route, Target};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
enum RecordedEvent {
    RequestStart(RequestStartEvent),
    AttemptStart(AttemptStartEvent),
    AttemptSkipped(AttemptSkippedEvent),
    AttemptFailure(AttemptFailureEvent),
    AttemptSuccess(AttemptSuccessEvent),
    RequestEnd(RequestEndEvent),
}

impl RecordedEvent {
    fn name(&self) -> &'static str {
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
struct RecordingObserver {
    events: Mutex<Vec<RecordedEvent>>,
}

impl RecordingObserver {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    fn snapshot(&self) -> Vec<RecordedEvent> {
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

fn event_names(events: &[RecordedEvent]) -> Vec<&'static str> {
    events.iter().map(RecordedEvent::name).collect()
}

fn as_attempt_skipped(event: &RecordedEvent) -> &AttemptSkippedEvent {
    match event {
        RecordedEvent::AttemptSkipped(inner) => inner,
        other => panic!("expected attempt_skipped event, got {}", other.name()),
    }
}

#[tokio::test]
async fn direct_streaming_yields_envelopes_and_finishes_with_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello from stream\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield")
        .expect("first item should be ok");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(completion.meta.status_code, Some(200));
    assert_eq!(completion.meta.request_id.as_deref(), Some("req_sse"));
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello from stream")]
    );
}

#[tokio::test]
async fn direct_streaming_finish_after_drain_returns_completion() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"drained response\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open");

    while next_stream_item(&mut stream).await.is_some() {}

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("drained response")]
    );
}

#[tokio::test]
async fn routed_streaming_happy_path_finishes_with_response_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello from route\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openai_default())),
        )
        .await
        .expect("stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield")
        .expect("stream item should succeed");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.attempts.len(), 1);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello from route")]
    );
}

#[tokio::test]
async fn routed_streaming_retries_next_target_when_initial_stream_open_fails() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"openai/gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"fallback stream\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"openai/gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url("http://127.0.0.1:1")
                .with_default_model("gpt-5-mini"),
        )
        .with_openrouter(
            crate::ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("openai/gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(crate::ProviderInstanceId::openrouter_default()))
                .with_fallback_policy(crate::FallbackPolicy::new().with_rule(
                    crate::FallbackRule::retry_on_kind(crate::RuntimeErrorKind::Transport),
                )),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("fallback stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield")
        .expect("stream item should succeed");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(
        completion.meta.selected_provider_kind,
        ProviderKind::OpenRouter
    );
    assert_eq!(completion.meta.attempts.len(), 2);
    assert_eq!(
        completion.meta.attempts[0].provider_kind,
        ProviderKind::OpenAi
    );
    assert!(matches!(
        completion.meta.attempts[0].disposition,
        AttemptDisposition::Failed { .. }
    ));
    assert_eq!(
        completion.meta.attempts[1].provider_kind,
        ProviderKind::OpenRouter
    );
    assert!(matches!(
        completion.meta.attempts[1].disposition,
        AttemptDisposition::Succeeded { .. }
    ));
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("fallback stream")]
    );
}

#[tokio::test]
async fn routed_streaming_allows_fallback_after_raw_envelope_without_canonical_events() {
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"recovered after raw frame\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let primary_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "data: keep-alive before any canonical event\n\n",
            "data: {\"missing\":\"type\"}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openrouter(
            crate::ProviderConfig::new("test-key")
                .with_base_url(primary_url)
                .with_default_model("openai/gpt-5-mini"),
        )
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(fallback_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openrouter_default()))
                .with_fallback(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_fallback_policy(crate::FallbackPolicy::new().with_rule(
                    crate::FallbackRule::retry_on_kind(crate::RuntimeErrorKind::ProtocolViolation),
                )),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield raw setup envelope")
        .expect("setup envelope should not fail");
    assert!(first.canonical.is_empty());

    let second = next_stream_item(&mut stream)
        .await
        .expect("fallback stream should yield")
        .expect("fallback stream item should succeed");
    assert!(!second.canonical.is_empty());

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.attempts.len(), 2);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("recovered after raw frame")]
    );
}

#[tokio::test]
async fn routed_streaming_explicit_task_api_uses_route_and_execution_options() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"explicit route stream\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let task = MessageCreateInput::user("hello explicit route")
        .into_task_request()
        .expect("task request should build");
    let route = Route::to(
        Target::new(crate::ProviderInstanceId::openai_default()).with_model("gpt-5-mini"),
    );
    let execution = ExecutionOptions {
        response_mode: crate::ResponseMode::Streaming,
        ..ExecutionOptions::default()
    };

    let mut stream = toolkit
        .streaming()
        .execute(task, route, execution)
        .await
        .expect("explicit routed stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield")
        .expect("stream item should succeed");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("explicit route stream")]
    );
}

#[tokio::test]
async fn routed_streaming_does_not_fallback_after_first_canonical_event() {
    let primary_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"error\":{\"message\":\"stream failed after commit\"},\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\",\"model\":\"openai/gpt-5-mini\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_2\",\"delta\":\"should never be used\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_2\",\"model\":\"openai/gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let observer = Arc::new(RecordingObserver::new());
    let toolkit = AgentToolkit::builder()
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(primary_url)
                .with_default_model("gpt-5-mini"),
        )
        .with_openrouter(
            crate::ProviderConfig::new("test-key")
                .with_base_url(fallback_url)
                .with_default_model("openai/gpt-5-mini"),
        )
        .observer(observer.clone())
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(crate::ProviderInstanceId::openrouter_default()))
                .with_fallback_policy(crate::FallbackPolicy::new().with_rule(
                    crate::FallbackRule::retry_on_kind(crate::RuntimeErrorKind::Upstream),
                )),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield canonical event")
        .expect("first canonical event should succeed");
    assert!(!first.canonical.is_empty());

    let terminal = next_stream_item(&mut stream)
        .await
        .expect("stream should surface provider terminal event")
        .expect("committed stream should stay on the active attempt");
    assert_eq!(
        terminal.canonical,
        vec![CanonicalStreamEvent::Failed {
            message: "stream failed after commit".to_string(),
        }]
    );

    let finish_error = stream
        .finish()
        .await
        .expect_err("finish should return the committed-stream error");
    assert_eq!(finish_error.kind, RuntimeErrorKind::Upstream);
    assert!(finish_error.message.contains("stream failed after commit"));
    let failure_meta = executed_failure_meta(&finish_error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        crate::ProviderInstanceId::openai_default()
    );
    assert_eq!(failure_meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(failure_meta.selected_model, "gpt-5-mini");
    assert_eq!(failure_meta.status_code, Some(200));
    assert_eq!(failure_meta.request_id.as_deref(), Some("req_sse"));
    assert_eq!(failure_meta.attempts.len(), 1);
    assert!(matches!(
        failure_meta.attempts[0].disposition,
        crate::AttemptDisposition::Failed {
            error_kind: crate::RuntimeErrorKind::Upstream,
            ..
        }
    ));

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
}

#[tokio::test]
async fn routed_streaming_terminal_error_carries_ordered_attempt_history() {
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_2\",\"model\":\"gpt-5-mini\",\"error\":{\"message\":\"fallback stream failed after commit\"},\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let primary_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "data: keep-alive before any canonical event\n\n",
            "data: {\"missing\":\"type\"}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openrouter(
            crate::ProviderConfig::new("test-key")
                .with_base_url(primary_url)
                .with_default_model("openai/gpt-5-mini"),
        )
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(fallback_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openrouter_default()))
                .with_fallback(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_fallback_policy(crate::FallbackPolicy::new().with_rule(
                    crate::FallbackRule::retry_on_kind(crate::RuntimeErrorKind::ProtocolViolation),
                )),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("primary stream should yield")
        .expect("primary setup envelope should not fail");
    assert!(first.canonical.is_empty());

    let second = next_stream_item(&mut stream)
        .await
        .expect("fallback stream should yield")
        .expect("fallback canonical event should succeed");
    assert!(!second.canonical.is_empty());

    let terminal = next_stream_item(&mut stream)
        .await
        .expect("fallback stream should surface terminal event")
        .expect("terminal envelope should stay on fallback attempt");
    assert_eq!(
        terminal.canonical,
        vec![CanonicalStreamEvent::Failed {
            message: "fallback stream failed after commit".to_string(),
        }]
    );

    let finish_error = stream
        .finish()
        .await
        .expect_err("finish should return the fallback attempt error");
    let failure_meta = executed_failure_meta(&finish_error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        crate::ProviderInstanceId::openai_default()
    );
    assert_eq!(failure_meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(failure_meta.selected_model, "gpt-5-mini");
    assert_eq!(failure_meta.attempts.len(), 2);
    assert_eq!(
        failure_meta.attempts[0].provider_instance,
        crate::ProviderInstanceId::openrouter_default()
    );
    assert!(matches!(
        failure_meta.attempts[0].disposition,
        crate::AttemptDisposition::Failed {
            error_kind: crate::RuntimeErrorKind::ProtocolViolation,
            ..
        }
    ));
    assert_eq!(
        failure_meta.attempts[1].provider_instance,
        crate::ProviderInstanceId::openai_default()
    );
    assert!(matches!(
        failure_meta.attempts[1].disposition,
        crate::AttemptDisposition::Failed {
            error_kind: crate::RuntimeErrorKind::Upstream,
            ..
        }
    ));
}

#[tokio::test]
async fn routed_streaming_terminal_error_keeps_pre_open_failures_and_skips_before_open_success() {
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_3\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_3\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_3\",\"delta\":\"opened after retry\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_3\",\"model\":\"gpt-5-mini\",\"error\":{\"message\":\"final stream failed after retry\"},\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                crate::ProviderInstanceId::openrouter_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenRouter,
                    "http://127.0.0.1:1",
                    Some("openai/gpt-5-mini"),
                ),
            ),
            (
                crate::ProviderInstanceId::openai_default(),
                test_provider_client_with_streaming_support(
                    ProviderKind::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                crate::ProviderInstanceId::generic_openai_compatible_default(),
                test_provider_client_with_base_url(
                    ProviderKind::GenericOpenAiCompatible,
                    &fallback_url,
                    Some("gpt-5-mini"),
                ),
            ),
        ]),
        observer: None,
    };

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openrouter_default()))
                .with_fallback(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(
                    crate::ProviderInstanceId::generic_openai_compatible_default(),
                ))
                .with_fallback_policy(crate::FallbackPolicy::new().with_rule(
                    crate::FallbackRule::retry_on_kind(crate::RuntimeErrorKind::Transport),
                ))
                .with_planning_rejection_policy(
                    crate::PlanningRejectionPolicy::SkipRejectedTargets,
                ),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("third target should open after a failed open and a skipped target");

    let first = next_stream_item(&mut stream)
        .await
        .expect("opened stream should yield")
        .expect("canonical event should succeed");
    assert!(!first.canonical.is_empty());

    let terminal = loop {
        let envelope = next_stream_item(&mut stream)
            .await
            .expect("stream should keep yielding until the terminal event")
            .expect("terminal envelopes should stay on the opened attempt");
        if matches!(
            envelope.canonical.as_slice(),
            [CanonicalStreamEvent::Failed { .. }]
        ) {
            break envelope;
        }
    };
    assert_eq!(
        terminal.canonical,
        vec![CanonicalStreamEvent::Failed {
            message: "final stream failed after retry".to_string(),
        }]
    );

    let finish_error = stream
        .finish()
        .await
        .expect_err("finish should return the committed terminal failure");
    let failure_meta = executed_failure_meta(&finish_error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        crate::ProviderInstanceId::generic_openai_compatible_default()
    );
    assert_eq!(
        failure_meta.selected_provider_kind,
        ProviderKind::GenericOpenAiCompatible
    );
    assert_eq!(failure_meta.attempts.len(), 3);
    assert_eq!(
        failure_meta.attempts[0].provider_instance,
        crate::ProviderInstanceId::openrouter_default()
    );
    assert!(matches!(
        failure_meta.attempts[0].disposition,
        crate::AttemptDisposition::Failed {
            error_kind: crate::RuntimeErrorKind::Transport,
            ..
        }
    ));
    assert_eq!(
        failure_meta.attempts[1].provider_instance,
        crate::ProviderInstanceId::openai_default()
    );
    assert!(matches!(
        failure_meta.attempts[1].disposition,
        crate::AttemptDisposition::Skipped {
            reason: crate::SkipReason::StaticIncompatibility { .. },
        }
    ));
    assert_eq!(
        failure_meta.attempts[2].provider_instance,
        crate::ProviderInstanceId::generic_openai_compatible_default()
    );
    assert!(matches!(
        failure_meta.attempts[2].disposition,
        crate::AttemptDisposition::Failed {
            error_kind: crate::RuntimeErrorKind::Upstream,
            ..
        }
    ));
}

#[tokio::test]
async fn routed_streaming_success_uses_typed_attempt_history_for_legacy_response_meta() {
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_4\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_4\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_4\",\"delta\":\"opened after typed history\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_4\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_4\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                crate::ProviderInstanceId::openrouter_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenRouter,
                    "http://127.0.0.1:1",
                    Some("openai/gpt-5-mini"),
                ),
            ),
            (
                crate::ProviderInstanceId::openai_default(),
                test_provider_client_with_streaming_support(
                    ProviderKind::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                crate::ProviderInstanceId::generic_openai_compatible_default(),
                test_provider_client_with_base_url(
                    ProviderKind::GenericOpenAiCompatible,
                    &fallback_url,
                    Some("gpt-5-mini"),
                ),
            ),
        ]),
        observer: None,
    };

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openrouter_default()))
                .with_fallback(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(
                    crate::ProviderInstanceId::generic_openai_compatible_default(),
                ))
                .with_fallback_policy(crate::FallbackPolicy::new().with_rule(
                    crate::FallbackRule::retry_on_kind(crate::RuntimeErrorKind::Transport),
                ))
                .with_planning_rejection_policy(
                    crate::PlanningRejectionPolicy::SkipRejectedTargets,
                ),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("third target should open after a failed open and a skipped target");

    let first = next_stream_item(&mut stream)
        .await
        .expect("opened stream should yield")
        .expect("canonical event should succeed");
    assert!(!first.canonical.is_empty());

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(
        completion.meta.selected_provider_kind,
        ProviderKind::GenericOpenAiCompatible
    );
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(completion.meta.attempts.len(), 3);
    assert_eq!(
        completion.meta.attempts[0].provider_kind,
        ProviderKind::OpenRouter
    );
    assert!(matches!(
        completion.meta.attempts[0].disposition,
        AttemptDisposition::Failed { .. }
    ));
    assert_eq!(
        completion.meta.attempts[1].provider_kind,
        ProviderKind::OpenAi
    );
    assert!(matches!(
        completion.meta.attempts[1].disposition,
        AttemptDisposition::Skipped { .. }
    ));
    assert_eq!(
        completion.meta.attempts[2].provider_kind,
        ProviderKind::GenericOpenAiCompatible
    );
    assert!(matches!(
        completion.meta.attempts[2].disposition,
        AttemptDisposition::Succeeded { .. }
    ));
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("opened after typed history")]
    );
}

#[tokio::test]
async fn routed_streaming_fail_fast_stops_on_planning_rejection_before_fallback() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                crate::ProviderInstanceId::openai_default(),
                test_provider_client_with_streaming_support(
                    ProviderKind::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                crate::ProviderInstanceId::openrouter_default(),
                test_provider_client(ProviderKind::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let error = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(crate::ProviderInstanceId::openrouter_default()))
                .with_planning_rejection_policy(crate::PlanningRejectionPolicy::FailFast),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect_err("planning rejection must stop before fallback");

    assert_eq!(error.kind, crate::RuntimeErrorKind::TargetResolution);
    let failure = route_planning_failure(&error);
    assert_eq!(
        failure.reason,
        crate::RoutePlanningFailureReason::NoCompatibleAttempts
    );
    assert_eq!(failure.attempts.len(), 1);
    assert_eq!(failure.attempts[0].model, "gpt-5-mini");
    assert!(matches!(
        failure.attempts[0].disposition,
        crate::AttemptDisposition::Skipped {
            reason: crate::SkipReason::StaticIncompatibility { .. }
        }
    ));
}

#[tokio::test]
async fn routed_streaming_emits_attempt_skipped_without_execution_events_for_skipped_target() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"openai/gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"stream fallback success\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"openai/gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let observer = Arc::new(RecordingObserver::new());
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                crate::ProviderInstanceId::openai_default(),
                test_provider_client_with_streaming_support(
                    ProviderKind::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                crate::ProviderInstanceId::openrouter_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenRouter,
                    &base_url,
                    Some("openai/gpt-5-mini"),
                ),
            ),
        ]),
        observer: Some(observer.clone()),
    };

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(crate::ProviderInstanceId::openrouter_default()))
                .with_planning_rejection_policy(
                    crate::PlanningRejectionPolicy::SkipRejectedTargets,
                ),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should skip incompatible attempt and open fallback");

    while next_stream_item(&mut stream).await.is_some() {}
    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(
        completion.meta.selected_provider_kind,
        ProviderKind::OpenRouter
    );

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec![
            "request_start",
            "attempt_skipped",
            "attempt_start",
            "attempt_success",
            "request_end",
        ]
    );

    let skipped = as_attempt_skipped(&events[1]);
    assert_eq!(
        skipped.provider_instance,
        crate::ProviderInstanceId::openai_default()
    );
    assert_eq!(skipped.provider_kind, ProviderKind::OpenAi);
    assert_eq!(skipped.model, "gpt-5-mini");
    assert_eq!(skipped.target_index, 0);
    assert_eq!(skipped.attempt_index, 0);
    assert!(matches!(
        skipped.reason,
        crate::SkipReason::StaticIncompatibility { .. }
    ));
}

#[tokio::test]
async fn routed_streaming_planning_failure_emits_request_end_after_attempt_skipped() {
    let observer = Arc::new(RecordingObserver::new());
    let toolkit = AgentToolkit {
        clients: HashMap::from([(
            crate::ProviderInstanceId::openai_default(),
            test_provider_client_with_streaming_support(
                ProviderKind::OpenAi,
                Some("gpt-5-mini"),
                false,
            ),
        )]),
        observer: Some(observer.clone()),
    };

    let error = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openai_default()))
                .with_planning_rejection_policy(crate::PlanningRejectionPolicy::FailFast),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect_err("planning rejection must stop routing");

    assert_eq!(error.kind, crate::RuntimeErrorKind::TargetResolution);

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec!["request_start", "attempt_skipped", "request_end"]
    );

    let skipped = as_attempt_skipped(&events[1]);
    assert_eq!(
        skipped.provider_instance,
        crate::ProviderInstanceId::openai_default()
    );
    assert_eq!(skipped.provider_kind, ProviderKind::OpenAi);
    assert_eq!(skipped.model, "gpt-5-mini");
    assert!(matches!(
        skipped.reason,
        crate::SkipReason::StaticIncompatibility { .. }
    ));
}

#[tokio::test]
async fn direct_text_stream_yields_text_chunks_and_finishes_with_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello \"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"world\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield")
            .expect("first text item should succeed"),
        "hello "
    );
    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield second delta")
            .expect("second text item should succeed"),
        "world"
    );

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello world")]
    );
}

#[tokio::test]
async fn routed_text_stream_yields_text_chunks_and_finishes_with_response_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello from route\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(crate::ProviderInstanceId::openai_default())),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield")
            .expect("text item should succeed"),
        "hello from route"
    );

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.attempts.len(), 1);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello from route")]
    );
}

#[test]
fn text_stream_enqueues_multiple_text_deltas_from_one_envelope_in_order() {
    let mut pending = std::collections::VecDeque::new();

    crate::message_text_stream::MessageTextStream::enqueue_text_deltas(
        &mut pending,
        CanonicalStreamEnvelope {
            raw: agent_core::ProviderRawStreamEvent::from_sse(
                ProviderKind::OpenAi,
                1,
                Some("response.synthetic".to_string()),
                None,
                None,
                r#"{"type":"response.synthetic"}"#,
            ),
            canonical: vec![
                CanonicalStreamEvent::ResponseStarted {
                    model: Some("gpt-5-mini".to_string()),
                    response_id: Some("resp_1".to_string()),
                },
                CanonicalStreamEvent::TextDelta {
                    output_index: 0,
                    content_index: 0,
                    item_id: Some("msg_1".to_string()),
                    delta: "hello ".to_string(),
                },
                CanonicalStreamEvent::TextDelta {
                    output_index: 0,
                    content_index: 1,
                    item_id: Some("msg_1".to_string()),
                    delta: "world".to_string(),
                },
            ],
        },
    );

    assert_eq!(
        pending.into_iter().collect::<Vec<_>>(),
        vec!["hello ".to_string(), "world".to_string()]
    );
}

#[tokio::test]
async fn text_stream_skips_non_text_envelopes_until_text_arrives() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"after setup\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield")
            .expect("text item should succeed"),
        "after setup"
    );
}

#[tokio::test]
async fn text_stream_finish_after_partial_consumption_preserves_full_response() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello \"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"again\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("first text chunk should be available")
            .expect("first text chunk should succeed"),
        "hello "
    );

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello again")]
    );
}

#[tokio::test]
async fn text_stream_surfaces_terminal_error_after_emitting_prior_text() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"partial text\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"error\":{\"message\":\"stream failed late\"},\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield")
            .expect("first text item should succeed"),
        "partial text"
    );

    let poll_error = next_text_stream_item(&mut stream)
        .await
        .expect("text stream should surface terminal error")
        .expect_err("terminal item should be an error");
    assert_eq!(poll_error.kind, RuntimeErrorKind::Upstream);
    assert!(poll_error.message.contains("stream failed late"));

    let finish_error = stream
        .finish()
        .await
        .expect_err("finish should return the same error");
    assert_eq!(finish_error.kind, RuntimeErrorKind::Upstream);
    assert_eq!(finish_error.message, poll_error.message);
    let failure_meta = executed_failure_meta(&finish_error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        crate::ProviderInstanceId::openai_default()
    );
    assert_eq!(failure_meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(failure_meta.selected_model, "gpt-5-mini");
    assert_eq!(failure_meta.attempts.len(), 1);
}

#[tokio::test]
async fn text_stream_completion_matches_envelope_stream_completion() {
    let body = concat!(
        "event: response.created\n",
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"same \"}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"response\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
    );
    let base_url_one = spawn_sse_stub("text/event-stream", body).await;
    let base_url_two = spawn_sse_stub("text/event-stream", body).await;
    let envelope_client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url_one, Some("gpt-5-mini"));
    let text_client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url_two, Some("gpt-5-mini"));

    let mut envelope_stream = envelope_client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("envelope stream should open");
    while next_stream_item(&mut envelope_stream).await.is_some() {}
    let envelope_completion = envelope_stream
        .finish()
        .await
        .expect("envelope completion should succeed");

    let mut text_stream = text_client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("text stream should open")
        .into_text_stream();
    while next_text_stream_item(&mut text_stream).await.is_some() {}
    let text_completion = text_stream
        .finish()
        .await
        .expect("text completion should succeed");

    assert_eq!(text_completion.response, envelope_completion.response);
    assert_eq!(text_completion.meta, envelope_completion.meta);
}

#[tokio::test]
async fn text_stream_finish_after_drain_returns_completion() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"finish after drain\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    while next_text_stream_item(&mut stream).await.is_some() {}

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("finish after drain")]
    );
}

async fn next_stream_item(
    stream: &mut crate::MessageResponseStream,
) -> Option<Result<agent_core::CanonicalStreamEnvelope, crate::RuntimeError>> {
    stream.next().await
}

async fn next_text_stream_item(
    stream: &mut crate::MessageTextStream,
) -> Option<Result<String, crate::RuntimeError>> {
    stream.next().await
}

fn test_streaming_provider_client(
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

async fn spawn_sse_stub(content_type: &str, body: &str) -> String {
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
