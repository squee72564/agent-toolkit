use super::*;

#[test]
fn terminal_failure_error_returns_underlying_for_fallback_exhausted() {
    let terminal = RuntimeError {
        kind: RuntimeErrorKind::Upstream,
        message: "terminal upstream error".to_string(),
        provider: Some(ProviderId::OpenAi),
        status_code: Some(503),
        request_id: Some("req_terminal".to_string()),
        provider_code: Some("rate_limit_exceeded".to_string()),
        source: None,
    };

    let wrapped = RuntimeError::fallback_exhausted(terminal);
    let extracted = terminal_failure_error(&wrapped);

    assert_eq!(extracted.kind, RuntimeErrorKind::Upstream);
    assert_eq!(extracted.status_code, Some(503));
    assert_eq!(extracted.request_id.as_deref(), Some("req_terminal"));
}
