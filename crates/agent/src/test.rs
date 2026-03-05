use super::*;

#[test]
fn provider_id_reexport_matches_agent_core_type() {
    let provider_from_agent: ProviderId = ProviderId::OpenAi;
    let provider_from_core: agent_core::types::ProviderId = provider_from_agent;
    assert_eq!(provider_from_core, agent_core::types::ProviderId::OpenAi);
}

#[test]
fn observability_reexports_are_accessible() {
    fn assert_runtime_observer_type<T: RuntimeObserver>() {}
    assert_runtime_observer_type::<NoopObserver>();

    let _ = RequestStartEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: std::time::Duration::from_millis(0),
        first_target: None,
        resolved_target_count: 0,
    };
    let _ = AttemptStartEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: std::time::Duration::from_millis(0),
    };
    let _ = AttemptSuccessEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: std::time::Duration::from_millis(0),
        status_code: None,
    };
    let _ = AttemptFailureEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: std::time::Duration::from_millis(0),
        error_kind: None,
        error_message: None,
    };
    let _ = RequestEndEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: std::time::Duration::from_millis(0),
        status_code: None,
        error_kind: None,
        error_message: None,
    };
}

#[derive(Debug)]
struct NoopObserver;

impl RuntimeObserver for NoopObserver {}
