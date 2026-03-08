use agent_core::ProviderId;
use agent_core::stream::{ProviderRawStreamEvent, RawStreamPayload};
use serde_json::json;

#[test]
fn from_sse_classifies_json_payloads() {
    let event = ProviderRawStreamEvent::from_sse(
        ProviderId::OpenAi,
        7,
        Some("response.created".to_string()),
        Some("evt_123".to_string()),
        Some(250),
        r#"{"type":"response.created"}"#,
    );

    assert_eq!(event.provider, ProviderId::OpenAi);
    assert_eq!(event.sequence, 7);
    assert_eq!(event.sse_event_name(), Some("response.created"));
    assert_eq!(event.json(), Some(&json!({"type": "response.created"})));
    assert!(matches!(event.payload, RawStreamPayload::Json { .. }));
}

#[test]
fn from_sse_classifies_text_payloads() {
    let event = ProviderRawStreamEvent::from_sse(
        ProviderId::Anthropic,
        3,
        Some("message_delta".to_string()),
        None,
        None,
        "not-json",
    );

    assert!(matches!(
        event.payload,
        RawStreamPayload::Text { ref text } if text == "not-json"
    ));
}

#[test]
fn from_sse_classifies_done_payloads() {
    let event =
        ProviderRawStreamEvent::from_sse(ProviderId::OpenRouter, 1, None, None, None, "[DONE]");

    assert!(matches!(event.payload, RawStreamPayload::Done));
}

#[test]
fn from_sse_classifies_empty_payloads() {
    let event = ProviderRawStreamEvent::from_sse(ProviderId::OpenRouter, 2, None, None, None, "");

    assert!(matches!(event.payload, RawStreamPayload::Empty));
}
