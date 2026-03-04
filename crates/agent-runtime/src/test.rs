use std::collections::HashMap;

use super::*;

#[test]
fn message_input_from_str_creates_user_message() {
    let input = MessageCreateInput::from("hello");
    assert_eq!(input.messages.len(), 1);
    assert_eq!(input.messages[0].role, MessageRole::User);
}

#[test]
fn fallback_policy_matches_transport_or_retryable_status() {
    let policy = FallbackPolicy::default();

    let transport_error = RuntimeError {
        kind: RuntimeErrorKind::Transport,
        message: "transport".to_string(),
        provider: Some(ProviderId::OpenAi),
        status_code: None,
        request_id: None,
        provider_code: None,
        source: None,
    };
    assert!(policy.should_fallback(&transport_error));

    let rate_limit_error = RuntimeError {
        kind: RuntimeErrorKind::Upstream,
        message: "rate limited".to_string(),
        provider: Some(ProviderId::OpenAi),
        status_code: Some(429),
        request_id: None,
        provider_code: None,
        source: None,
    };
    assert!(policy.should_fallback(&rate_limit_error));
}

#[test]
fn router_requires_explicit_target_without_policy() {
    let toolkit = AgentToolkit {
        clients: HashMap::new(),
    };
    let error = toolkit
        .resolve_targets(&SendOptions::default())
        .expect_err("target resolution should fail");
    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn message_input_uses_default_model_when_missing() {
    let request = MessageCreateInput::from("hello")
        .into_request_with_options(Some("default-model"), false)
        .expect("default model should be used");
    assert_eq!(request.model_id, "default-model");
}

#[test]
fn message_input_allows_empty_model_for_router_path() {
    let request = MessageCreateInput::from("hello")
        .into_request_with_options(None, true)
        .expect("empty model should be allowed for router path");
    assert!(request.model_id.is_empty());
}
