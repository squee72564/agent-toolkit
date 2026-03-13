use std::collections::BTreeMap;

use agent_core::{
    FamilyOptions, Message, NativeOptions, OpenAiCompatibleOptions, OpenAiOptions, ProviderOptions,
    Request, ResponseFormat, ToolChoice,
};

use crate::route_planning::{self, PlanningRejectionKind, PrepareAttemptError};
use crate::{
    AttemptExecutionOptions, AttemptSpec, PlanningRejectionPolicy, ProviderConfig, Target,
};

use super::*;

#[test]
fn prepare_route_attempt_rejects_mismatched_native_family() {
    let client = test_provider_client(agent_core::ProviderId::OpenAi);
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi)).with_execution(
        AttemptExecutionOptions::default().with_native_options(NativeOptions {
            family: Some(FamilyOptions::Anthropic(
                agent_core::AnthropicFamilyOptions { thinking: None },
            )),
            provider: None,
        }),
    );

    let error = route_planning::prepare_route_attempt(
        &client,
        &attempt,
        crate::ResponseMode::NonStreaming,
        &test_request("gpt-5-mini"),
    )
    .expect_err("family mismatch must reject");

    match error {
        PrepareAttemptError::Rejected(rejection) => {
            assert_eq!(rejection.kind, PlanningRejectionKind::StaticIncompatibility);
            assert!(
                rejection.error.message.contains("native family options"),
                "unexpected message: {}",
                rejection.error.message
            );
        }
        other => panic!("expected rejected attempt, got {other:?}"),
    }
}

#[test]
fn prepare_route_attempt_rejects_mismatched_native_provider() {
    let client = test_provider_client(agent_core::ProviderId::OpenAi);
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi)).with_execution(
        AttemptExecutionOptions::default().with_native_options(NativeOptions {
            family: None,
            provider: Some(ProviderOptions::Anthropic(agent_core::AnthropicOptions {
                top_k: Some(8),
            })),
        }),
    );

    let error = route_planning::prepare_route_attempt(
        &client,
        &attempt,
        crate::ResponseMode::NonStreaming,
        &test_request("gpt-5-mini"),
    )
    .expect_err("provider mismatch must reject");

    match error {
        PrepareAttemptError::Rejected(rejection) => {
            assert_eq!(rejection.kind, PlanningRejectionKind::StaticIncompatibility);
            assert!(
                rejection.error.message.contains("native provider options"),
                "unexpected message: {}",
                rejection.error.message
            );
        }
        other => panic!("expected rejected attempt, got {other:?}"),
    }
}

#[test]
fn prepare_route_attempt_rejects_provider_native_layer_for_generic_openai_compatible() {
    let client = test_provider_client(agent_core::ProviderId::GenericOpenAiCompatible);
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::GenericOpenAiCompatible))
        .with_execution(
            AttemptExecutionOptions::default().with_native_options(NativeOptions {
                family: Some(FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
                    parallel_tool_calls: Some(true),
                    reasoning: None,
                })),
                provider: Some(ProviderOptions::OpenAi(OpenAiOptions {
                    service_tier: Some("priority".to_string()),
                    store: Some(true),
                })),
            }),
        );

    let error = route_planning::prepare_route_attempt(
        &client,
        &attempt,
        crate::ResponseMode::NonStreaming,
        &test_request("gpt-5-mini"),
    )
    .expect_err("unsupported provider native layer must reject");

    match error {
        PrepareAttemptError::Rejected(rejection) => {
            assert_eq!(rejection.kind, PlanningRejectionKind::StaticIncompatibility);
            assert!(
                rejection.error.message.contains("native provider options"),
                "unexpected message: {}",
                rejection.error.message
            );
        }
        other => panic!("expected rejected attempt, got {other:?}"),
    }
}

#[test]
fn prepare_route_attempt_rejects_streaming_when_provider_capability_is_disabled() {
    let client =
        test_provider_client_with_streaming_support(agent_core::ProviderId::OpenAi, None, false);
    let attempt =
        AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi).with_model("gpt-5-mini"));

    let error = route_planning::prepare_route_attempt(
        &client,
        &attempt,
        crate::ResponseMode::Streaming,
        &test_request("gpt-5-mini"),
    )
    .expect_err("streaming capability mismatch must reject");

    match error {
        PrepareAttemptError::Rejected(rejection) => {
            assert_eq!(rejection.kind, PlanningRejectionKind::StaticIncompatibility);
            assert!(
                rejection
                    .error
                    .message
                    .contains("does not support streaming"),
                "unexpected message: {}",
                rejection.error.message
            );
        }
        other => panic!("expected rejected attempt, got {other:?}"),
    }
}

#[test]
fn skip_rejection_policy_advances_only_when_more_attempts_exist() {
    assert!(route_planning::should_skip_planning_rejection(
        PlanningRejectionPolicy::SkipRejectedTargets,
        0,
        2,
    ));
    assert!(!route_planning::should_skip_planning_rejection(
        PlanningRejectionPolicy::SkipRejectedTargets,
        1,
        2,
    ));
    assert!(!route_planning::should_skip_planning_rejection(
        PlanningRejectionPolicy::FailFast,
        0,
        2,
    ));
}

#[test]
fn prepare_route_attempt_uses_target_model_before_provider_default() {
    let client =
        provider_client_with_default_model(agent_core::ProviderId::OpenAi, Some("default-model"));
    let attempt =
        AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi).with_model("target-model"));

    let prepared = route_planning::prepare_route_attempt(
        &client,
        &attempt,
        crate::ResponseMode::NonStreaming,
        &test_request("request-model"),
    )
    .expect("preflight must succeed");

    assert_eq!(prepared.selected_model, "target-model");
}

#[test]
fn prepare_route_attempt_uses_provider_default_when_target_and_request_models_are_blank() {
    let client =
        provider_client_with_default_model(agent_core::ProviderId::OpenAi, Some("default-model"));
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi));

    let prepared = route_planning::prepare_route_attempt(
        &client,
        &attempt,
        crate::ResponseMode::NonStreaming,
        &test_request(" "),
    )
    .expect("preflight must succeed");

    assert_eq!(prepared.selected_model, "default-model");
}

#[test]
fn prepare_route_attempt_treats_missing_model_as_fatal() {
    let client = provider_client_with_default_model(agent_core::ProviderId::OpenAi, None);
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi));

    let error = route_planning::prepare_route_attempt(
        &client,
        &attempt,
        crate::ResponseMode::NonStreaming,
        &test_request(" "),
    )
    .expect_err("missing model must fail");

    match error {
        PrepareAttemptError::Fatal(error) => {
            assert_eq!(error.kind, RuntimeErrorKind::Configuration);
            assert!(error.message.contains("no model available"));
        }
        other => panic!("expected fatal attempt error, got {other:?}"),
    }
}

#[test]
fn prepare_route_attempt_classifies_adapter_planning_rejection() {
    let client = provider_client_with_default_model(
        agent_core::ProviderId::Anthropic,
        Some("claude-sonnet-4-6"),
    );
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::Anthropic));
    let request = Request {
        model_id: String::new(),
        stream: false,
        messages: vec![
            Message::user_text("hello"),
            Message::system_text("late system"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: BTreeMap::new(),
    };

    let error = route_planning::prepare_route_attempt(
        &client,
        &attempt,
        crate::ResponseMode::NonStreaming,
        &request,
    )
    .expect_err("anthropic planning should reject non-prefix system messages");

    match error {
        PrepareAttemptError::Rejected(rejection) => {
            assert_eq!(
                rejection.kind,
                PlanningRejectionKind::AdapterPlanningRejected
            );
            assert_eq!(rejection.error.kind, RuntimeErrorKind::Validation);
            assert!(
                rejection.error.message.contains("contiguous prefix"),
                "unexpected message: {}",
                rejection.error.message
            );
        }
        other => panic!("expected rejected attempt, got {other:?}"),
    }
}

fn provider_client_with_default_model(
    provider: agent_core::ProviderId,
    default_model: Option<&str>,
) -> ProviderClient {
    let adapter = agent_providers::adapter::adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = agent_transport::HttpTransport::builder(client).build();
    let platform = adapter
        .platform_config("http://127.0.0.1:1".to_string())
        .expect("test platform should build");
    let instance_id = Target::default_instance_for(provider);
    let mut config = ProviderConfig::new("test-key");
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = RegisteredProvider::new(instance_id.clone(), provider, config);

    ProviderClient::new(ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    })
}

fn test_request(model_id: &str) -> Request {
    Request {
        model_id: model_id.to_string(),
        stream: false,
        messages: vec![Message::user_text("hello")],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: BTreeMap::new(),
    }
}
