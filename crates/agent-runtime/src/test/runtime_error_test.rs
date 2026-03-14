use super::*;
use agent_transport::{TimeoutStage, TransportError};

#[test]
fn runtime_error_clone_preserves_source_chain() {
    let terminal = RuntimeError {
        kind: RuntimeErrorKind::Upstream,
        message: "terminal upstream error".to_string(),
        provider: Some(ProviderKind::OpenAi),
        status_code: Some(503),
        request_id: Some("req_terminal".to_string()),
        provider_code: Some("rate_limit_exceeded".to_string()),
        executed_failure_meta: None,
        source: None,
    };

    let cloned = RuntimeError::fallback_exhausted(terminal).clone();
    let extracted = terminal_failure_error(&cloned);

    assert_eq!(cloned.kind, RuntimeErrorKind::FallbackExhausted);
    assert_eq!(extracted.kind, RuntimeErrorKind::Upstream);
    assert_eq!(extracted.status_code, Some(503));
    assert_eq!(extracted.request_id.as_deref(), Some("req_terminal"));
}

#[test]
fn terminal_failure_error_returns_underlying_for_fallback_exhausted() {
    let terminal = RuntimeError {
        kind: RuntimeErrorKind::Upstream,
        message: "terminal upstream error".to_string(),
        provider: Some(ProviderKind::OpenAi),
        status_code: Some(503),
        request_id: Some("req_terminal".to_string()),
        provider_code: Some("rate_limit_exceeded".to_string()),
        executed_failure_meta: None,
        source: None,
    };

    let wrapped = RuntimeError::fallback_exhausted(terminal);
    let extracted = terminal_failure_error(&wrapped);

    assert_eq!(extracted.kind, RuntimeErrorKind::Upstream);
    assert_eq!(extracted.status_code, Some(503));
    assert_eq!(extracted.request_id.as_deref(), Some("req_terminal"));
}

#[test]
fn transport_timeout_messages_preserve_stream_stage() {
    let first_byte = RuntimeError::from_transport(
        ProviderKind::OpenAi,
        TransportError::Timeout {
            stage: TimeoutStage::FirstByte,
        },
    );
    assert_eq!(first_byte.kind, RuntimeErrorKind::Transport);
    assert_eq!(first_byte.message, "stream first byte timed out");

    let stream_idle = RuntimeError::from_transport(
        ProviderKind::OpenAi,
        TransportError::Timeout {
            stage: TimeoutStage::StreamIdle,
        },
    );
    assert_eq!(stream_idle.message, "stream idle timed out");
}
