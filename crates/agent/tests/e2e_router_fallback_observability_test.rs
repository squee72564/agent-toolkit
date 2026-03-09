mod e2e;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_toolkit::{
    AgentToolkit, AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, FallbackMode,
    FallbackPolicy, FallbackRule, MessageCreateInput, ProviderConfig, ProviderId, RequestEndEvent,
    RequestStartEvent, RetryPolicy, RuntimeErrorKind, RuntimeObserver, SendOptions, Target,
};

use e2e::fixtures::{FixtureProvider, FixtureScenario, load_fixture_json};
use e2e::mock_server::{MockResponse, MockServer};
use e2e::timeout::with_test_timeout;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    RequestStart,
    AttemptStart,
    AttemptSuccess,
    AttemptFailure,
    RequestEnd,
}

#[derive(Debug, Default)]
struct RecordingObserver {
    events: Mutex<Vec<Event>>,
}

impl RecordingObserver {
    fn snapshot(&self) -> Vec<Event> {
        self.events.lock().expect("observer mutex").clone()
    }

    fn push(&self, event: Event) {
        self.events.lock().expect("observer mutex").push(event);
    }
}

impl RuntimeObserver for RecordingObserver {
    fn on_request_start(&self, _: &RequestStartEvent) {
        self.push(Event::RequestStart);
    }

    fn on_attempt_start(&self, _: &AttemptStartEvent) {
        self.push(Event::AttemptStart);
    }

    fn on_attempt_success(&self, _: &AttemptSuccessEvent) {
        self.push(Event::AttemptSuccess);
    }

    fn on_attempt_failure(&self, _: &AttemptFailureEvent) {
        self.push(Event::AttemptFailure);
    }

    fn on_request_end(&self, _: &RequestEndEvent) {
        self.push(Event::RequestEnd);
    }
}

#[tokio::test]
async fn fallback_retries_next_provider_on_status_rule_then_succeeds() {
    let openai_error = load_fixture_json(
        FixtureProvider::OpenAi,
        FixtureScenario::InvalidAuth,
        "gpt-5-mini.json",
    );
    let anthropic_ok = load_fixture_json(
        FixtureProvider::Anthropic,
        FixtureScenario::BasicChat,
        "claude-sonnet-4-6.json",
    );

    let openai_server = MockServer::spawn(vec![
        MockResponse::json(401, openai_error).with_header("x-request-id", "openai_401"),
    ])
    .await;
    let anthropic_server = MockServer::spawn(vec![
        MockResponse::json(200, anthropic_ok).with_header("request-id", "anthropic_200"),
    ])
    .await;

    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("openai-key")
                .with_base_url(openai_server.base_url())
                .with_default_model("gpt-5-mini"),
        )
        .with_anthropic(
            ProviderConfig::new("anthropic-key")
                .with_base_url(anthropic_server.base_url())
                .with_default_model("claude-sonnet-4-6"),
        )
        .build()
        .expect("build toolkit");

    let fallback = FallbackPolicy::new(vec![Target::new(ProviderId::Anthropic)])
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_status(401));

    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi).with_model("gpt-5-mini"))
        .with_fallback_policy(fallback);

    let (_response, meta) = with_test_timeout(
        toolkit
            .messages()
            .create_with_meta(MessageCreateInput::user("hello"), options),
    )
    .await
    .expect("fallback should succeed on anthropic");

    assert_eq!(meta.selected_provider, ProviderId::Anthropic);
    assert_eq!(meta.attempts.len(), 2);
    assert_eq!(meta.attempts[0].provider, ProviderId::OpenAi);
    assert!(!meta.attempts[0].success);
    assert_eq!(meta.attempts[1].provider, ProviderId::Anthropic);
    assert!(meta.attempts[1].success);
}

#[tokio::test]
async fn fallback_exhaustion_returns_terminal_error_kind() {
    let openai_error = load_fixture_json(
        FixtureProvider::OpenAi,
        FixtureScenario::InvalidAuth,
        "gpt-5-mini.json",
    );
    let anthropic_error = load_fixture_json(
        FixtureProvider::Anthropic,
        FixtureScenario::InvalidAuth,
        "claude-sonnet-4-5-20250929.json",
    );

    let openai_server = MockServer::spawn(vec![MockResponse::json(401, openai_error)]).await;
    let anthropic_server = MockServer::spawn(vec![MockResponse::json(401, anthropic_error)]).await;

    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("openai-key")
                .with_base_url(openai_server.base_url())
                .with_default_model("gpt-5-mini"),
        )
        .with_anthropic(
            ProviderConfig::new("anthropic-key")
                .with_base_url(anthropic_server.base_url())
                .with_default_model("claude-sonnet-4-5-20250929"),
        )
        .build()
        .expect("build toolkit");

    let fallback = FallbackPolicy::new(vec![Target::new(ProviderId::Anthropic)])
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_status(401));

    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi).with_model("gpt-5-mini"))
        .with_fallback_policy(fallback);

    let error = with_test_timeout(
        toolkit
            .messages()
            .create_with_meta(MessageCreateInput::user("hello"), options),
    )
    .await
    .expect_err("fallback should be exhausted");

    assert_eq!(error.kind, RuntimeErrorKind::FallbackExhausted);
    let terminal = error
        .source_ref()
        .and_then(|source| source.downcast_ref::<agent_toolkit::RuntimeError>())
        .expect("fallback exhausted should carry terminal runtime error source");
    assert_eq!(terminal.kind, RuntimeErrorKind::Upstream);
}

#[tokio::test]
async fn fallback_rule_retry_on_provider_code_is_honored() {
    let coded_error = serde_json::json!({
        "error": {
            "message": "rate limited",
            "code": "rate_limit_exceeded"
        }
    });
    let openrouter_ok = load_fixture_json(
        FixtureProvider::OpenRouter,
        FixtureScenario::BasicChat,
        "openai.gpt-5.4.json",
    );

    let openai_server = MockServer::spawn(vec![MockResponse::json(429, coded_error)]).await;
    let openrouter_server = MockServer::spawn(vec![
        MockResponse::json(200, openrouter_ok).with_header("x-request-id", "openrouter_success"),
    ])
    .await;

    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("openai-key")
                .with_base_url(openai_server.base_url())
                .with_default_model("gpt-5-mini")
                .with_retry_policy(RetryPolicy {
                    max_attempts: 1,
                    ..RetryPolicy::default()
                }),
        )
        .with_openrouter(
            ProviderConfig::new("openrouter-key")
                .with_base_url(openrouter_server.base_url())
                .with_default_model("openai.gpt-5.4"),
        )
        .build()
        .expect("build toolkit");

    let fallback = FallbackPolicy::new(vec![Target::new(ProviderId::OpenRouter)])
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_provider_code("rate_limit_exceeded"));

    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi).with_model("gpt-5-mini"))
        .with_fallback_policy(fallback);

    let (_response, meta) = with_test_timeout(
        toolkit
            .messages()
            .create_with_meta(MessageCreateInput::user("hello"), options),
    )
    .await
    .expect("provider-code fallback should succeed");

    assert_eq!(meta.selected_provider, ProviderId::OpenRouter);
    assert_eq!(meta.attempts.len(), 2);
}

#[tokio::test]
async fn send_observer_takes_precedence_over_toolkit_observer_and_records_ordered_callbacks() {
    let success = load_fixture_json(
        FixtureProvider::OpenAi,
        FixtureScenario::BasicChat,
        "gpt-5-mini.json",
    );

    let server = MockServer::spawn(vec![
        MockResponse::json(200, success).with_header("x-request-id", "obs_success"),
    ])
    .await;

    let toolkit_observer = Arc::new(RecordingObserver::default());
    let send_observer = Arc::new(RecordingObserver::default());

    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("openai-key")
                .with_base_url(server.base_url())
                .with_default_model("gpt-5-mini"),
        )
        .observer(toolkit_observer.clone())
        .build()
        .expect("build toolkit");

    let options = SendOptions {
        target: Some(Target::new(ProviderId::OpenAi).with_model("gpt-5-mini")),
        fallback_policy: None,
        metadata: Default::default(),
        observer: Some(send_observer.clone()),
    };

    let _ = with_test_timeout(
        toolkit
            .messages()
            .create_with_meta(MessageCreateInput::user("hello"), options),
    )
    .await
    .expect("request should succeed");

    assert_eq!(
        send_observer.snapshot(),
        vec![
            Event::RequestStart,
            Event::AttemptStart,
            Event::AttemptSuccess,
            Event::RequestEnd,
        ]
    );
    assert!(toolkit_observer.snapshot().is_empty());
}

#[tokio::test]
async fn observer_lifecycle_on_failure_attempts_is_deterministic() {
    let toolkit_observer = Arc::new(RecordingObserver::default());

    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("openai-key")
                .with_base_url("http://127.0.0.1:1")
                .with_default_model("gpt-5-mini"),
        )
        .observer(toolkit_observer.clone())
        .build()
        .expect("build toolkit");

    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi).with_model("gpt-5-mini"));

    let error = with_test_timeout(
        toolkit
            .messages()
            .create_with_meta(MessageCreateInput::user("hello"), options),
    )
    .await
    .expect_err("request should fail");

    assert_eq!(error.kind, RuntimeErrorKind::Transport);
    assert_eq!(
        toolkit_observer.snapshot(),
        vec![
            Event::RequestStart,
            Event::AttemptStart,
            Event::AttemptFailure,
            Event::RequestEnd,
        ]
    );

    tokio::time::sleep(Duration::from_millis(10)).await;
}
