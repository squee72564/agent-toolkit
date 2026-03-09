use agent_core::ProviderId;
use agent_transport::{HttpResponseHead, HttpResponseMode};
use reqwest::{StatusCode, header::HeaderMap};

use crate::RuntimeErrorKind;
use crate::provider_runtime::response_mode_mismatch_error;

#[test]
fn response_mode_mismatch_reports_protocol_violation_for_json_expectation() {
    let error = response_mode_mismatch_error(
        ProviderId::OpenAi,
        HttpResponseMode::Json,
        "SSE",
        &response_head(StatusCode::OK, Some("req_json_mismatch")),
    );

    assert_eq!(error.kind, RuntimeErrorKind::ProtocolViolation);
    assert_eq!(error.provider, Some(ProviderId::OpenAi));
    assert_eq!(error.status_code, Some(200));
    assert_eq!(error.request_id.as_deref(), Some("req_json_mismatch"));
    assert!(
        error.message.contains("expected JSON response, got SSE"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn response_mode_mismatch_reports_protocol_violation_for_sse_expectation() {
    let error = response_mode_mismatch_error(
        ProviderId::Anthropic,
        HttpResponseMode::Sse,
        "JSON",
        &response_head(StatusCode::CREATED, Some("req_sse_mismatch")),
    );

    assert_eq!(error.kind, RuntimeErrorKind::ProtocolViolation);
    assert_eq!(error.provider, Some(ProviderId::Anthropic));
    assert_eq!(error.status_code, Some(201));
    assert_eq!(error.request_id.as_deref(), Some("req_sse_mismatch"));
    assert!(
        error.message.contains("expected SSE response, got JSON"),
        "unexpected message: {}",
        error.message
    );
}

fn response_head(status: StatusCode, request_id: Option<&str>) -> HttpResponseHead {
    HttpResponseHead {
        status,
        headers: HeaderMap::new(),
        request_id: request_id.map(ToString::to_string),
    }
}
