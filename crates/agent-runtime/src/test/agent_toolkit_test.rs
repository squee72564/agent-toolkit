use std::sync::Arc;
use std::sync::Mutex;

use agent_core::ProviderInstanceId;

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

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

async fn spawn_json_success_stub(request_id: &str) -> String {
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

#[test]
fn builder_requires_at_least_one_provider() {
    let error = AgentToolkit::builder()
        .build()
        .expect_err("builder should require at least one provider");

    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
    assert_eq!(error.message, "at least one provider must be configured");
}

#[test]
fn builder_registers_openai_provider() {
    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("builder should register openai");

    assert!(
        toolkit
            .clients
            .contains_key(&Target::default_instance_for(ProviderId::OpenAi))
    );
}

#[test]
fn builder_registers_anthropic_provider() {
    let toolkit = AgentToolkit::builder()
        .with_anthropic(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("builder should register anthropic");

    assert!(
        toolkit
            .clients
            .contains_key(&Target::default_instance_for(ProviderId::Anthropic))
    );
}

#[test]
fn builder_registers_openrouter_provider() {
    let toolkit = AgentToolkit::builder()
        .with_openrouter(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("builder should register openrouter");

    assert!(
        toolkit
            .clients
            .contains_key(&Target::default_instance_for(ProviderId::OpenRouter))
    );
}

#[test]
fn builder_registers_custom_provider_instance() {
    let toolkit = AgentToolkit::builder()
        .with_openai_instance(
            "openai-secondary",
            ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"),
        )
        .build()
        .expect("builder should register named openai instance");

    assert!(
        toolkit
            .clients
            .contains_key(&ProviderInstanceId::new("openai-secondary"))
    );
}

#[test]
fn builder_supports_multiple_instances_for_same_provider_kind() {
    let toolkit = AgentToolkit::builder()
        .with_openai_instance(
            "openai-primary",
            ProviderConfig::new("primary-key").with_base_url("http://127.0.0.1:1"),
        )
        .with_openai_instance(
            "openai-secondary",
            ProviderConfig::new("secondary-key").with_base_url("http://127.0.0.1:1"),
        )
        .build()
        .expect("builder should register multiple openai instances");

    assert!(
        toolkit
            .clients
            .contains_key(&ProviderInstanceId::new("openai-primary"))
    );
    assert!(
        toolkit
            .clients
            .contains_key(&ProviderInstanceId::new("openai-secondary"))
    );
}

#[test]
fn builder_propagates_observer_to_provider_runtime() {
    let observer = Arc::new(ObserverStub);
    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .observer(observer.clone())
        .build()
        .expect("builder should register observer");

    let client = toolkit
        .clients
        .get(&Target::default_instance_for(ProviderId::OpenAi))
        .expect("openai client should be registered");

    assert!(toolkit.observer.is_some());
    assert!(client.runtime.observer.is_some());
}

#[test]
fn router_requires_explicit_target_without_policy() {
    let toolkit = AgentToolkit {
        clients: HashMap::new(),
        observer: None,
    };
    let error = toolkit
        .resolve_route_targets(&Route {
            primary: Target::new(ProviderId::OpenAi).into(),
            fallbacks: Vec::new(),
            fallback_policy: FallbackPolicy::default(),
            planning_rejection_policy: PlanningRejectionPolicy::FailFast,
        })
        .expect_err("target resolution should fail");
    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn resolve_route_targets_errors_for_unregistered_provider() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([(
            Target::default_instance_for(ProviderId::OpenAi),
            test_provider_client(ProviderId::OpenAi),
        )]),
        observer: None,
    };
    let error = toolkit
        .resolve_route_targets(&Route::to(
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
        ))
        .expect_err("unregistered provider should fail target resolution");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn resolve_route_targets_deduplicates_primary_and_fallback_targets() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(ProviderId::OpenAi),
                test_provider_client(ProviderId::OpenAi),
            ),
            (
                Target::default_instance_for(ProviderId::OpenRouter),
                test_provider_client(ProviderId::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let route = crate::Route::to(Target::new(ProviderId::OpenAi).with_model("gpt-5"))
        .with_fallbacks(vec![
            Target::new(ProviderId::OpenAi).with_model("gpt-5").into(),
            Target::new(ProviderId::OpenRouter)
                .with_model("openai/gpt-5")
                .into(),
            Target::new(ProviderId::OpenRouter)
                .with_model("openai/gpt-5")
                .into(),
        ]);

    let targets = toolkit
        .resolve_route_targets(&route)
        .expect("target resolution should succeed");

    assert_eq!(
        targets
            .into_iter()
            .map(|attempt| attempt.target)
            .collect::<Vec<_>>(),
        vec![
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
        ]
    );
}

#[test]
fn resolve_route_targets_preserves_attempt_order() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(ProviderId::OpenAi),
                test_provider_client(ProviderId::OpenAi),
            ),
            (
                Target::default_instance_for(ProviderId::OpenRouter),
                test_provider_client(ProviderId::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let route = crate::Route::to(Target::new(ProviderId::OpenAi).with_model("gpt-5"))
        .with_fallback(Target::new(ProviderId::OpenAi).with_model("gpt-5"))
        .with_fallback(Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"));

    let targets = toolkit
        .resolve_route_targets(&route)
        .expect("route target resolution should succeed");

    assert_eq!(
        targets
            .into_iter()
            .map(|attempt| attempt.target)
            .collect::<Vec<_>>(),
        vec![
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
        ]
    );
}

#[tokio::test]
async fn routed_messages_fail_fast_surfaces_typed_route_planning_failure() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(ProviderId::Anthropic),
                test_provider_client(ProviderId::Anthropic),
            ),
            (
                Target::default_instance_for(ProviderId::OpenRouter),
                test_provider_client(ProviderId::OpenRouter),
            ),
        ]),
        observer: None,
    };
    let task = MessageCreateInput::new(vec![
        Message::user_text("hello"),
        Message::system_text("late system"),
    ])
    .into_task_request()
    .expect("task request should build");

    let error = toolkit
        .messages()
        .execute(
            task,
            Route::to(Target::new(ProviderId::Anthropic).with_model("claude-sonnet-4-6"))
                .with_fallback(Target::new(ProviderId::OpenRouter))
                .with_planning_rejection_policy(PlanningRejectionPolicy::FailFast),
            ExecutionOptions::default(),
        )
        .await
        .expect_err("planning rejection must stop before fallback");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
    let failure = route_planning_failure(&error);
    assert_eq!(
        failure.reason,
        RoutePlanningFailureReason::AllAttemptsRejectedDuringPlanning
    );
    assert_eq!(failure.attempts.len(), 1);
    assert_eq!(failure.attempts[0].model, "claude-sonnet-4-6");
    assert!(matches!(
        failure.attempts[0].disposition,
        AttemptDisposition::Skipped {
            reason: SkipReason::AdapterPlanningRejected { .. }
        }
    ));
}

#[tokio::test]
async fn routed_messages_create_uses_explicit_provider_instance_route() {
    let base_url = spawn_json_success_stub("req_routed_custom_instance").await;
    let toolkit = AgentToolkit::builder()
        .with_openai_instance(
            "openai-primary",
            ProviderConfig::new("primary-key")
                .with_base_url("http://127.0.0.1:1")
                .with_default_model("gpt-4.1-mini"),
        )
        .with_openai_instance(
            "openai-secondary",
            ProviderConfig::new("secondary-key")
                .with_base_url(&base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("builder should register multiple openai instances");

    let (_response, meta) = toolkit
        .messages()
        .create_with_meta(
            MessageCreateInput::user("hello"),
            Route::to(Target::new("openai-secondary").with_model("gpt-5-mini")),
        )
        .await
        .expect("request should target the named provider instance");

    assert_eq!(meta.selected_provider, ProviderId::OpenAi);
}

#[tokio::test]
async fn routed_messages_emit_attempt_skipped_without_execution_events_for_planning_rejection() {
    let base_url = spawn_json_success_stub("req_routed_skip_success").await;
    let observer = Arc::new(RecordingObserver::new());
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(ProviderId::Anthropic),
                test_provider_client(ProviderId::Anthropic),
            ),
            (
                Target::default_instance_for(ProviderId::OpenAi),
                test_provider_client_with_base_url(
                    ProviderId::OpenAi,
                    &base_url,
                    Some("gpt-5-mini"),
                ),
            ),
        ]),
        observer: Some(observer.clone()),
    };

    let task = MessageCreateInput::new(vec![
        Message::user_text("hello"),
        Message::system_text("late system"),
    ])
    .into_task_request()
    .expect("task request should build");

    let (_response, meta) = toolkit
        .messages()
        .execute_with_meta(
            task,
            Route::to(Target::new(ProviderId::Anthropic).with_model("claude-sonnet-4-6"))
                .with_fallback(Target::new(ProviderId::OpenAi).with_model("gpt-5-mini"))
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
            ExecutionOptions::default(),
        )
        .await
        .expect("route should skip rejected attempt and succeed");

    assert_eq!(meta.selected_provider, ProviderId::OpenAi);

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
        Target::default_instance_for(ProviderId::Anthropic)
    );
    assert_eq!(skipped.provider_kind, ProviderId::Anthropic);
    assert_eq!(skipped.model, "claude-sonnet-4-6");
    assert_eq!(skipped.target_index, 0);
    assert_eq!(skipped.attempt_index, 0);
    assert!(skipped.elapsed >= std::time::Duration::ZERO);
    assert!(matches!(
        skipped.reason,
        SkipReason::AdapterPlanningRejected { .. }
    ));
}
