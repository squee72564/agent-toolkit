#![cfg(feature = "live-tests")]

mod e2e;

use std::sync::{Arc, Mutex};

use agent_toolkit::{
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, MessageCreateInput, ProviderId,
    RequestEndEvent, RequestStartEvent, RuntimeObserver, SendOptions, Target, openai,
};

use e2e::live::{maybe_observer_event_count, provider_api_key};
use e2e::timeout::with_test_timeout;

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

#[tokio::test]
async fn live_openai_streaming_emits_observer_lifecycle_events() {
    let Some(api_key) = provider_api_key(ProviderId::OpenAi) else {
        eprintln!("skipping live OpenAI observer test: OPENAI_API_KEY is not set");
        return;
    };

    let observer = Arc::new(RecordingObserver::default());
    let observer_trait: Arc<dyn RuntimeObserver> = observer.clone();

    let client = openai()
        .api_key(api_key)
        .default_model("gpt-5-mini")
        .observer(observer_trait)
        .build()
        .expect("build openai client");

    let stream = with_test_timeout(
        client.streaming().create(
            MessageCreateInput::user("Say hello in five words."),
        ),
    )
    .await
    .expect("stream should open");

    let completion = with_test_timeout(stream.into_text_stream().finish())
        .await
        .expect("stream should finish");

    assert!(
        !completion.response.output.content.is_empty(),
        "expected finalized response content"
    );

    let events = observer.snapshot();
    assert!(
        events.contains(&"request_start"),
        "expected request_start event in observer trace"
    );
    assert!(
        events.contains(&"attempt_start"),
        "expected attempt_start event in observer trace"
    );
    assert!(
        events.contains(&"request_end"),
        "expected request_end event in observer trace"
    );
    assert!(
        events.contains(&"attempt_success") || events.contains(&"attempt_failure"),
        "expected terminal attempt event in observer trace"
    );

    let _ = maybe_observer_event_count(observer.as_ref());
}

#[tokio::test]
async fn live_openai_routed_streaming_supports_per_call_observer_override() {
    let Some(api_key) = provider_api_key(ProviderId::OpenAi) else {
        eprintln!("skipping live OpenAI routed observer test: OPENAI_API_KEY is not set");
        return;
    };

    let toolkit = agent_toolkit::AgentToolkit::builder()
        .with_openai(
            agent_toolkit::ProviderConfig::new(api_key).with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("build toolkit");

    let observer = Arc::new(RecordingObserver::default());
    let observer_trait: Arc<dyn RuntimeObserver> = observer.clone();

    let stream = with_test_timeout(toolkit.streaming().create(
        MessageCreateInput::user("Say hello in five words."),
        SendOptions::for_target(Target::new(ProviderId::OpenAi)).with_observer(observer_trait),
    ))
    .await
    .expect("routed stream should open");

    let completion = with_test_timeout(stream.into_text_stream().finish())
        .await
        .expect("routed stream should finish");

    assert_eq!(completion.meta.selected_provider, ProviderId::OpenAi);

    let events = observer.snapshot();
    assert!(
        events.contains(&"request_start"),
        "expected request_start event in override observer trace"
    );
    assert!(
        events.contains(&"request_end"),
        "expected request_end event in override observer trace"
    );
}
