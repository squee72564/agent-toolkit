use std::time::Duration;

use agent_providers::AdapterErrorKind;

use super::runtime::{
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent,
    RequestStartEvent, RuntimeObserver, StreamCompletion,
};
use super::transport::HttpJsonResponse;
use super::{core, message, request, response, runtime, tools, transport};

#[test]
fn provider_id_reexport_matches_agent_core_type() {
    for provider_from_agent in [
        core::ProviderKind::OpenAi,
        core::ProviderKind::Anthropic,
        core::ProviderKind::OpenRouter,
    ] {
        let provider_from_core: agent_core::types::ProviderKind = provider_from_agent;
        let expected = match provider_from_agent {
            core::ProviderKind::OpenAi => agent_core::types::ProviderKind::OpenAi,
            core::ProviderKind::Anthropic => agent_core::types::ProviderKind::Anthropic,
            core::ProviderKind::OpenRouter => agent_core::types::ProviderKind::OpenRouter,
            core::ProviderKind::GenericOpenAiCompatible => {
                agent_core::types::ProviderKind::GenericOpenAiCompatible
            }
        };

        assert_eq!(provider_from_core, expected);
    }
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
        elapsed: Duration::ZERO,
        first_target: None,
        resolved_target_count: 0,
    };
    let _ = AttemptStartEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: Duration::ZERO,
    };
    let _ = AttemptSuccessEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: Duration::ZERO,
        status_code: None,
    };
    let _ = AttemptFailureEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: Duration::ZERO,
        error_kind: None,
        error_message: None,
    };
    let _ = RequestEndEvent {
        request_id: None,
        provider: None,
        model: None,
        target_index: None,
        attempt_index: None,
        elapsed: Duration::ZERO,
        status_code: None,
        error_kind: None,
        error_message: None,
    };
}

#[test]
fn module_reexports_are_accessible() {
    let provider_from_core_mod: core::types::ProviderKind = core::types::ProviderKind::Anthropic;
    assert_eq!(
        provider_from_core_mod,
        agent_core::types::ProviderKind::Anthropic
    );

    let adapter_error_kind: AdapterErrorKind = AdapterErrorKind::Validation;
    assert_eq!(
        adapter_error_kind,
        agent_providers::AdapterErrorKind::Validation
    );

    let _default_retry_from_transport_mod = transport::RetryPolicy::default();
    let _runtime_error_kind = runtime::RuntimeErrorKind::Validation;
    let _tool_registry = tools::ToolRegistry::new();

    let _message_role = message::MessageRole::User;
    let _platform_provider = core::types::ProviderKind::OpenAi;
    let _response_format = request::ResponseFormat::default();
    let _finish_reason = response::FinishReason::Stop;
    let _tool_choice = tools::ToolChoice::Auto;
}

#[test]
fn top_level_transport_reexports_are_constructible() {
    fn assert_debug_clone<T: std::fmt::Debug + Clone>() {}

    assert_debug_clone::<HttpJsonResponse>();
    assert_debug_clone::<transport::RetryPolicy>();

    let retry = transport::RetryPolicy::default();
    assert_eq!(retry.max_attempts, 3);
    assert_eq!(retry.initial_backoff, Duration::from_millis(100));
    assert_eq!(retry.max_backoff, Duration::from_millis(2_000));
}

#[test]
fn streaming_reexports_are_accessible() {
    fn assert_streaming_type<T>() {}
    fn assert_text_conversion(
        stream: runtime::MessageResponseStream,
    ) -> runtime::MessageTextStream {
        stream.into_text_stream()
    }

    assert_streaming_type::<runtime::DirectStreamingApi<'static>>();
    assert_streaming_type::<runtime::RoutedStreamingApi<'static>>();
    assert_streaming_type::<runtime::MessageResponseStream>();
    assert_streaming_type::<runtime::MessageTextStream>();
    assert_streaming_type::<StreamCompletion>();
    let _ = assert_text_conversion;
}

#[derive(Debug)]
struct NoopObserver;

impl RuntimeObserver for NoopObserver {}
