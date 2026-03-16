use agent_core::{ProviderKind, RawStreamPayload, RawStreamTransport};
use agent_transport::SseEvent;

use crate::provider_stream_runtime::ProviderStreamRuntime;

#[test]
fn wrap_sse_event_assigns_monotonic_sequences() {
    let mut runtime = ProviderStreamRuntime::new(ProviderKind::OpenAi);

    let first = runtime.wrap_sse_event(SseEvent {
        event: Some("response.created".to_string()),
        data: r#"{"type":"response.created"}"#.to_string(),
        id: Some("evt-1".to_string()),
        retry: Some(10),
    });
    let second = runtime.wrap_sse_event(SseEvent {
        event: Some("response.completed".to_string()),
        data: "[DONE]".to_string(),
        id: Some("evt-2".to_string()),
        retry: None,
    });

    assert_eq!(first.sequence, 1);
    assert_eq!(second.sequence, 2);
}

#[test]
fn wrap_sse_event_preserves_transport_metadata_and_payload_shape() {
    let mut runtime = ProviderStreamRuntime::new(ProviderKind::Anthropic);

    let raw = runtime.wrap_sse_event(SseEvent {
        event: Some("message_start".to_string()),
        data: r#"{"message":{"id":"msg_1"}}"#.to_string(),
        id: Some("sse-1".to_string()),
        retry: Some(250),
    });

    assert_eq!(raw.provider, ProviderKind::Anthropic);
    assert_eq!(
        raw.transport,
        RawStreamTransport::Sse {
            event: Some("message_start".to_string()),
            id: Some("sse-1".to_string()),
            retry: Some(250),
        }
    );
    assert!(matches!(raw.payload, RawStreamPayload::Json { .. }));
}
