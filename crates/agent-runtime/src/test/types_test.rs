use std::time::Duration;

use super::*;
use crate::types::{
    RequestEndContext, attempt_failure_event_fields, attempt_success_event_fields,
    normalized_event_model, request_end_failure_event, request_end_success_event, response_meta,
};

#[test]
fn normalized_event_model_prefers_trimmed_target_model() {
    assert_eq!(
        normalized_event_model(Some("  gpt-5-mini  "), "request-model"),
        Some("gpt-5-mini".to_string())
    );
}

#[test]
fn normalized_event_model_falls_back_to_trimmed_request_model() {
    assert_eq!(
        normalized_event_model(Some("   "), " request-model "),
        Some("request-model".to_string())
    );
    assert_eq!(normalized_event_model(None, "   "), None);
}

#[test]
fn attempt_event_helpers_map_attempt_meta() {
    let success = attempt_success_event_fields(
        ProviderId::OpenAi,
        Some("gpt-5-mini".to_string()),
        Some("req_123".to_string()),
        2,
        3,
        Duration::from_secs(1),
        Some(200),
    );
    assert_eq!(success.request_id.as_deref(), Some("req_123"));
    assert_eq!(success.provider, Some(ProviderId::OpenAi));
    assert_eq!(success.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(success.target_index, Some(2));
    assert_eq!(success.attempt_index, Some(3));
    assert_eq!(success.status_code, Some(200));

    let failure = attempt_failure_event_fields(
        ProviderId::OpenAi,
        Some("gpt-5-mini".to_string()),
        Some("req_123".to_string()),
        2,
        3,
        Duration::from_secs(1),
        Some(RuntimeErrorKind::Upstream),
        Some("upstream failure".to_string()),
    );
    assert_eq!(failure.request_id.as_deref(), Some("req_123"));
    assert_eq!(failure.provider, Some(ProviderId::OpenAi));
    assert_eq!(failure.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(failure.target_index, Some(2));
    assert_eq!(failure.attempt_index, Some(3));
    assert_eq!(failure.error_kind, Some(RuntimeErrorKind::Upstream));
    assert_eq!(failure.error_message.as_deref(), Some("upstream failure"));
}

#[test]
fn request_end_event_helpers_map_terminal_outcomes() {
    let success = request_end_success_event(RequestEndContext {
        request_id: Some("req_123".to_string()),
        provider: Some(ProviderId::OpenAi),
        model: Some("gpt-5-mini".to_string()),
        target_index: Some(0),
        attempt_index: Some(0),
        elapsed: Duration::from_secs(2),
        status_code: Some(200),
    });
    assert_eq!(success.error_kind, None);
    assert_eq!(success.error_message, None);
    assert_eq!(success.status_code, Some(200));

    let failure = request_end_failure_event(
        RequestEndContext {
            request_id: Some("req_999".to_string()),
            provider: Some(ProviderId::Anthropic),
            model: Some("claude".to_string()),
            target_index: Some(1),
            attempt_index: Some(1),
            elapsed: Duration::from_secs(3),
            status_code: Some(503),
        },
        RuntimeErrorKind::Transport,
        "timed out".to_string(),
    );
    assert_eq!(failure.request_id.as_deref(), Some("req_999"));
    assert_eq!(failure.provider, Some(ProviderId::Anthropic));
    assert_eq!(failure.model.as_deref(), Some("claude"));
    assert_eq!(failure.status_code, Some(503));
    assert_eq!(failure.error_kind, Some(RuntimeErrorKind::Transport));
    assert_eq!(failure.error_message.as_deref(), Some("timed out"));
}

#[test]
fn response_meta_helper_preserves_selected_attempt_and_order() {
    let first = AttemptRecord {
        provider_instance: crate::ProviderInstanceId::anthropic_default(),
        provider_kind: ProviderId::Anthropic,
        model: "claude".to_string(),
        target_index: 0,
        attempt_index: 0,
        disposition: AttemptDisposition::Skipped {
            reason: SkipReason::AdapterPlanningRejected {
                message: "planning rejected".to_string(),
            },
        },
    };
    let second = AttemptRecord {
        provider_instance: crate::ProviderInstanceId::openai_default(),
        provider_kind: ProviderId::OpenAi,
        model: "gpt-5-mini".to_string(),
        target_index: 1,
        attempt_index: 1,
        disposition: AttemptDisposition::Succeeded {
            status_code: Some(200),
            request_id: Some("req_123".to_string()),
        },
    };
    let meta = response_meta(
        second.provider_instance.clone(),
        second.provider_kind,
        second.model.clone(),
        Some(200),
        Some("req_123".to_string()),
        vec![first.clone(), second.clone()],
    );

    assert_eq!(
        meta.selected_provider_instance,
        crate::ProviderInstanceId::openai_default()
    );
    assert_eq!(meta.selected_provider_kind, ProviderId::OpenAi);
    assert_eq!(meta.selected_model, "gpt-5-mini");
    assert_eq!(meta.request_id.as_deref(), Some("req_123"));
    assert_eq!(meta.attempts, vec![first, second]);
}
