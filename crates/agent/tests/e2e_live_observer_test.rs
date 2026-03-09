#![cfg(feature = "live-tests")]

mod e2e;

use std::sync::{Arc, Mutex};

use agent_toolkit::{
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, MessageCreateInput, ProviderId,
    RequestEndEvent, RequestStartEvent, RuntimeObserver, SendOptions, Target, openai,
};

use e2e::live::{
    assert_live_response_meta, default_live_model, require_provider_api_key, response_text,
    with_live_test_timeout,
};

#[derive(Debug, Default)]
struct RecordingObserver {
    events: Mutex<Vec<&'static str>>,
}

impl RecordingObserver {
    fn snapshot(&self) -> Vec<&'static str> {
        self.events.lock().expect("observer mutex").clone()
    }

    fn push(&self, event: &'static str) {
        self.events.lock().expect("observer mutex").push(event);
    }
}

impl RuntimeObserver for RecordingObserver {
    fn on_request_start(&self, _: &RequestStartEvent) {
        self.push("request_start");
    }

    fn on_attempt_start(&self, _: &AttemptStartEvent) {
        self.push("attempt_start");
    }

    fn on_attempt_success(&self, _: &AttemptSuccessEvent) {
        self.push("attempt_success");
    }

    fn on_attempt_failure(&self, _: &AttemptFailureEvent) {
        self.push("attempt_failure");
    }

    fn on_request_end(&self, _: &RequestEndEvent) {
        self.push("request_end");
    }
}

fn assert_stream_observer_lifecycle(events: &[&'static str]) {
    assert!(
        matches!(events.first(), Some(&"request_start")),
        "expected request_start to be the first observer event, got {events:?}"
    );
    assert!(
        matches!(events.last(), Some(&"request_end")),
        "expected request_end to be the last observer event, got {events:?}"
    );

    let request_start_count = events
        .iter()
        .filter(|event| **event == "request_start")
        .count();
    let request_end_count = events
        .iter()
        .filter(|event| **event == "request_end")
        .count();
    let attempt_start_count = events
        .iter()
        .filter(|event| **event == "attempt_start")
        .count();
    let terminal_attempt_count = events
        .iter()
        .filter(|event| matches!(**event, "attempt_success" | "attempt_failure"))
        .count();

    assert_eq!(request_start_count, 1, "expected one request_start event");
    assert_eq!(request_end_count, 1, "expected one request_end event");
    assert!(
        attempt_start_count >= 1,
        "expected at least one attempt_start event"
    );
    assert!(
        terminal_attempt_count >= 1,
        "expected at least one terminal attempt event"
    );
    assert!(
        events
            .iter()
            .position(|event| *event == "attempt_start")
            .is_some_and(|index| index > 0),
        "expected attempt_start after request_start"
    );
    assert!(
        events
            .iter()
            .rposition(|event| matches!(*event, "attempt_success" | "attempt_failure"))
            .is_some_and(|index| index < events.len() - 1),
        "expected a terminal attempt event before request_end"
    );
}

#[tokio::test]
async fn live_openai_streaming_emits_observer_lifecycle_events() {
    let Some(api_key) = require_provider_api_key(ProviderId::OpenAi, "live OpenAI observer test")
    else {
        return;
    };

    let observer = Arc::new(RecordingObserver::default());
    let observer_trait: Arc<dyn RuntimeObserver> = observer.clone();

    let client = openai()
        .api_key(api_key)
        .default_model(default_live_model(ProviderId::OpenAi))
        .observer(observer_trait)
        .build()
        .expect("build openai client");

    let stream = with_live_test_timeout(
        client
            .streaming()
            .create(MessageCreateInput::user("Say hello in five words.")),
    )
    .await
    .expect("stream should open");

    let completion = with_live_test_timeout(stream.into_text_stream().finish())
        .await
        .expect("stream should finish");

    assert_live_response_meta(&completion.meta, ProviderId::OpenAi);
    assert!(
        !response_text(&completion.response.output.content)
            .trim()
            .is_empty(),
        "expected finalized response content"
    );

    let events = observer.snapshot();
    assert_stream_observer_lifecycle(&events);
}

#[tokio::test]
async fn live_openai_routed_streaming_supports_per_call_observer_override() {
    let Some(api_key) =
        require_provider_api_key(ProviderId::OpenAi, "live OpenAI routed observer test")
    else {
        return;
    };

    let toolkit_observer = Arc::new(RecordingObserver::default());
    let toolkit_observer_trait: Arc<dyn RuntimeObserver> = toolkit_observer.clone();
    let toolkit = agent_toolkit::AgentToolkit::builder()
        .with_openai(
            agent_toolkit::ProviderConfig::new(api_key)
                .with_default_model(default_live_model(ProviderId::OpenAi)),
        )
        .observer(toolkit_observer_trait)
        .build()
        .expect("build toolkit");

    let observer = Arc::new(RecordingObserver::default());
    let observer_trait: Arc<dyn RuntimeObserver> = observer.clone();

    let stream = with_live_test_timeout(toolkit.streaming().create(
        MessageCreateInput::user("Say hello in five words."),
        SendOptions::for_target(Target::new(ProviderId::OpenAi)).with_observer(observer_trait),
    ))
    .await
    .expect("routed stream should open");

    let completion = with_live_test_timeout(stream.into_text_stream().finish())
        .await
        .expect("routed stream should finish");

    assert_live_response_meta(&completion.meta, ProviderId::OpenAi);
    assert!(
        !response_text(&completion.response.output.content)
            .trim()
            .is_empty(),
        "expected finalized routed response content"
    );
    assert!(
        toolkit_observer.snapshot().is_empty(),
        "expected per-call observer override to take precedence over the toolkit observer"
    );

    let events = observer.snapshot();
    assert_stream_observer_lifecycle(&events);
}
