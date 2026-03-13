use std::collections::BTreeMap;

use agent_core::{
    FamilyOptions, Message, NativeOptions, OpenAiCompatibleOptions, OpenAiOptions, ProviderOptions,
    Request, ResponseFormat, ToolChoice,
};

use crate::planner::{self, AttemptPlanningError, PlanningRejectionKind};
use crate::{AttemptExecutionOptions, AttemptSpec, ExecutionOptions, ProviderConfig, Target};

use super::*;

#[test]
fn routed_planner_rejects_mismatched_native_family() {
    let client = test_provider_client(agent_core::ProviderId::OpenAi);
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi).with_model("gpt-5"))
        .with_execution(
            AttemptExecutionOptions::default().with_native_options(NativeOptions {
                family: Some(FamilyOptions::Anthropic(
                    agent_core::AnthropicFamilyOptions { thinking: None },
                )),
                provider: None,
            }),
        );

    let error = planner::plan_routed_attempt(
        &client,
        &attempt,
        &test_request().task_request(),
        &ExecutionOptions::default(),
    )
    .expect_err("family mismatch must reject");

    match error {
        AttemptPlanningError::Rejected(rejection) => {
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
fn routed_planner_rejects_mismatched_native_provider() {
    let client = test_provider_client(agent_core::ProviderId::OpenAi);
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi).with_model("gpt-5"))
        .with_execution(
            AttemptExecutionOptions::default().with_native_options(NativeOptions {
                family: None,
                provider: Some(ProviderOptions::Anthropic(agent_core::AnthropicOptions {
                    top_k: Some(8),
                })),
            }),
        );

    let error = planner::plan_routed_attempt(
        &client,
        &attempt,
        &test_request().task_request(),
        &ExecutionOptions::default(),
    )
    .expect_err("provider mismatch must reject");

    match error {
        AttemptPlanningError::Rejected(rejection) => {
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
fn routed_planner_rejects_provider_native_layer_for_generic_openai_compatible() {
    let client = test_provider_client(agent_core::ProviderId::GenericOpenAiCompatible);
    let attempt = AttemptSpec::to(
        Target::new(agent_core::ProviderId::GenericOpenAiCompatible).with_model("gpt-5"),
    )
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

    let error = planner::plan_routed_attempt(
        &client,
        &attempt,
        &test_request().task_request(),
        &ExecutionOptions::default(),
    )
    .expect_err("unsupported provider native layer must reject");

    match error {
        AttemptPlanningError::Rejected(rejection) => {
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
fn routed_planner_rejects_streaming_when_provider_capability_is_disabled() {
    let client =
        test_provider_client_with_streaming_support(agent_core::ProviderId::OpenAi, None, false);
    let attempt =
        AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi).with_model("gpt-5-mini"));
    let execution = ExecutionOptions {
        response_mode: crate::ResponseMode::Streaming,
        ..ExecutionOptions::default()
    };

    let error = planner::plan_routed_attempt(
        &client,
        &attempt,
        &test_request().task_request(),
        &execution,
    )
    .expect_err("streaming capability mismatch must reject");

    match error {
        AttemptPlanningError::Rejected(rejection) => {
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
    assert!(planner::should_skip_planning_rejection(
        PlanningRejectionPolicy::SkipRejectedTargets,
        0,
        2,
    ));
    assert!(!planner::should_skip_planning_rejection(
        PlanningRejectionPolicy::SkipRejectedTargets,
        1,
        2,
    ));
    assert!(!planner::should_skip_planning_rejection(
        PlanningRejectionPolicy::FailFast,
        0,
        2,
    ));
}

#[test]
fn routed_planner_uses_target_model_before_provider_default() {
    let client =
        provider_client_with_default_model(agent_core::ProviderId::OpenAi, Some("default-model"));
    let attempt =
        AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi).with_model("target-model"));

    let plan = planner::plan_routed_attempt(
        &client,
        &attempt,
        &test_request().task_request(),
        &ExecutionOptions::default(),
    )
    .expect("planning must succeed");

    assert_eq!(plan.provider_attempt.model, "target-model");
}

#[test]
fn routed_planner_uses_provider_default_when_target_model_is_blank() {
    let client =
        provider_client_with_default_model(agent_core::ProviderId::OpenAi, Some("default-model"));
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi));

    let plan = planner::plan_routed_attempt(
        &client,
        &attempt,
        &test_request().task_request(),
        &ExecutionOptions::default(),
    )
    .expect("planning must succeed");

    assert_eq!(plan.provider_attempt.model, "default-model");
}

#[test]
fn routed_planner_treats_missing_model_as_fatal() {
    let client = provider_client_with_default_model(agent_core::ProviderId::OpenAi, None);
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi));

    let error = planner::plan_routed_attempt(
        &client,
        &attempt,
        &test_request().task_request(),
        &ExecutionOptions::default(),
    )
    .expect_err("missing model must fail");

    match error {
        AttemptPlanningError::Fatal(error) => {
            assert_eq!(error.kind, RuntimeErrorKind::Configuration);
            assert!(error.message.contains("no model available"));
        }
        other => panic!("expected fatal planning error, got {other:?}"),
    }
}

#[test]
fn routed_planner_classifies_adapter_planning_rejection() {
    let client = provider_client_with_default_model(
        agent_core::ProviderId::Anthropic,
        Some("claude-sonnet-4-6"),
    );
    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::Anthropic));
    let task = Request {
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
    }
    .task_request();

    let error =
        planner::plan_routed_attempt(&client, &attempt, &task, &ExecutionOptions::default())
            .expect_err("anthropic planning should reject non-prefix system messages");

    match error {
        AttemptPlanningError::Rejected(rejection) => {
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

#[test]
fn direct_planner_resolves_platform_auth_and_transport() {
    let client =
        provider_client_with_default_model(agent_core::ProviderId::OpenAi, Some("default-model"));
    let mut execution = ExecutionOptions::default();
    execution.transport.request_id_header_override = Some("x-custom-request-id".to_string());
    execution
        .transport
        .extra_headers
        .insert("x-route".to_string(), "route".to_string());

    let plan =
        planner::plan_direct_attempt(&client, &test_request().task_request(), None, &execution)
            .expect("planning must succeed");

    assert_eq!(
        plan.provider_attempt.instance_id,
        client.runtime.instance_id
    );
    assert_eq!(plan.platform.base_url, "http://127.0.0.1:1");
    assert_eq!(
        plan.auth.credentials,
        Some(agent_core::AuthCredentials::Token("test-key".to_string()))
    );
    assert_eq!(
        plan.transport.request_id_header_override.as_deref(),
        Some("x-custom-request-id")
    );
    assert_eq!(
        plan.transport
            .route_extra_headers
            .get("x-route")
            .map(String::as_str),
        Some("route")
    );
}

#[test]
fn routed_planning_failure_tracks_static_skip_history_and_reason() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(agent_core::ProviderId::OpenAi),
                test_provider_client_with_streaming_support(
                    agent_core::ProviderId::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                Target::default_instance_for(agent_core::ProviderId::OpenRouter),
                test_provider_client_with_streaming_support(
                    agent_core::ProviderId::OpenRouter,
                    Some("openai/gpt-5-mini"),
                    false,
                ),
            ),
        ]),
        observer: None,
    };
    let attempts = toolkit
        .resolve_route_targets(
            &Route::to(Target::new(agent_core::ProviderId::OpenAi))
                .with_fallback(Target::new(agent_core::ProviderId::OpenRouter))
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
        )
        .expect("route targets should resolve");
    let execution = ExecutionOptions {
        response_mode: crate::ResponseMode::Streaming,
        ..ExecutionOptions::default()
    };

    let result = planner::plan_routed_execution(
        &toolkit,
        &attempts,
        &test_request().task_request(),
        &execution,
        PlanningRejectionPolicy::SkipRejectedTargets,
    );

    match result {
        planner::RoutedPlanningResult::PlanningFailure(failure) => {
            assert_eq!(
                failure.reason,
                crate::RoutePlanningFailureReason::NoCompatibleAttempts
            );
            assert_eq!(failure.attempts.len(), 2);
            assert_eq!(
                failure.attempts[0].provider_instance,
                Target::default_instance_for(agent_core::ProviderId::OpenAi)
            );
            assert_eq!(
                failure.attempts[0].provider_kind,
                agent_core::ProviderId::OpenAi
            );
            assert_eq!(failure.attempts[0].model, "gpt-5-mini");
            assert_eq!(failure.attempts[0].target_index, 0);
            assert_eq!(failure.attempts[0].attempt_index, 0);
            assert!(matches!(
                failure.attempts[0].disposition,
                crate::AttemptDisposition::Skipped {
                    reason: crate::SkipReason::StaticIncompatibility { .. }
                }
            ));
            assert!(matches!(
                failure.attempts[1].disposition,
                crate::AttemptDisposition::Skipped {
                    reason: crate::SkipReason::StaticIncompatibility { .. }
                }
            ));
        }
        other => panic!("expected route planning failure, got {other:?}"),
    }
}

#[test]
fn routed_planning_failure_uses_adapter_rejection_reason_when_any_attempt_reaches_adapter_planning()
{
    let anthropic = provider_client_with_default_model(
        agent_core::ProviderId::Anthropic,
        Some("claude-sonnet-4-6"),
    );
    let openai = test_provider_client_with_streaming_support(
        agent_core::ProviderId::OpenAi,
        Some("gpt-5-mini"),
        false,
    );
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (anthropic.runtime.instance_id.clone(), anthropic),
            (openai.runtime.instance_id.clone(), openai),
        ]),
        observer: None,
    };
    let attempts = toolkit
        .resolve_route_targets(
            &Route::to(Target::new(agent_core::ProviderId::OpenAi))
                .with_fallback(Target::new(agent_core::ProviderId::Anthropic))
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
        )
        .expect("route targets should resolve");
    let execution = ExecutionOptions {
        response_mode: crate::ResponseMode::Streaming,
        ..ExecutionOptions::default()
    };
    let task = Request {
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
    }
    .task_request();

    let result = planner::plan_routed_execution(
        &toolkit,
        &attempts,
        &task,
        &execution,
        PlanningRejectionPolicy::SkipRejectedTargets,
    );

    match result {
        planner::RoutedPlanningResult::PlanningFailure(failure) => {
            assert_eq!(
                failure.reason,
                crate::RoutePlanningFailureReason::AllAttemptsRejectedDuringPlanning
            );
            assert_eq!(failure.attempts.len(), 2);
            assert!(matches!(
                failure.attempts[0].disposition,
                crate::AttemptDisposition::Skipped {
                    reason: crate::SkipReason::StaticIncompatibility { .. }
                }
            ));
            assert!(matches!(
                failure.attempts[1].disposition,
                crate::AttemptDisposition::Skipped {
                    reason: crate::SkipReason::AdapterPlanningRejected { .. }
                }
            ));
        }
        other => panic!("expected route planning failure, got {other:?}"),
    }
}

#[test]
fn routed_planning_fails_before_attempt_record_when_model_is_unresolved() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([(
            Target::default_instance_for(agent_core::ProviderId::OpenAi),
            provider_client_with_default_model(agent_core::ProviderId::OpenAi, None),
        )]),
        observer: None,
    };
    let attempts = toolkit
        .resolve_route_targets(&Route::to(Target::new(agent_core::ProviderId::OpenAi)))
        .expect("route targets should resolve");

    let result = planner::plan_routed_execution(
        &toolkit,
        &attempts,
        &test_request().task_request(),
        &ExecutionOptions::default(),
        PlanningRejectionPolicy::SkipRejectedTargets,
    );

    match result {
        planner::RoutedPlanningResult::Fatal(error) => {
            assert_eq!(error.kind, RuntimeErrorKind::Configuration);
            assert!(error.message.contains("no model available"));
        }
        other => panic!("expected fatal planning error, got {other:?}"),
    }
}

/// Phase 03 locked rule: skipped planning records must never carry provider
/// request-id or executed-attempt status metadata.  Only the disposition
/// fields appropriate for a planning-only skip should be present.
#[test]
fn skipped_planning_records_never_carry_request_id_or_status_metadata() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(agent_core::ProviderId::OpenAi),
                test_provider_client_with_streaming_support(
                    agent_core::ProviderId::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                Target::default_instance_for(agent_core::ProviderId::OpenRouter),
                test_provider_client_with_streaming_support(
                    agent_core::ProviderId::OpenRouter,
                    Some("openai/gpt-5-mini"),
                    false,
                ),
            ),
        ]),
        observer: None,
    };
    let attempts = toolkit
        .resolve_route_targets(
            &Route::to(Target::new(agent_core::ProviderId::OpenAi))
                .with_fallback(Target::new(agent_core::ProviderId::OpenRouter))
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
        )
        .expect("route targets should resolve");
    let execution = ExecutionOptions {
        response_mode: crate::ResponseMode::Streaming,
        ..ExecutionOptions::default()
    };

    let result = planner::plan_routed_execution(
        &toolkit,
        &attempts,
        &test_request().task_request(),
        &execution,
        PlanningRejectionPolicy::SkipRejectedTargets,
    );

    let failure = match result {
        planner::RoutedPlanningResult::PlanningFailure(failure) => failure,
        other => panic!("expected route planning failure, got {other:?}"),
    };

    for (i, record) in failure.attempts.iter().enumerate() {
        match &record.disposition {
            crate::AttemptDisposition::Skipped { reason: _ } => {
                // Skipped records must not carry execution-only metadata
                // (provider request-id or HTTP status code).  The type
                // guarantees this structurally: `Skipped` only holds a
                // `SkipReason`.  Assert that the variant is indeed `Skipped`
                // and does not match the executed dispositions which do carry
                // those fields.
                assert!(
                    !matches!(
                        record.disposition,
                        crate::AttemptDisposition::Succeeded { .. }
                            | crate::AttemptDisposition::Failed { .. }
                    ),
                    "skipped record [{i}] must not use an executed disposition"
                );
            }
            other => panic!("expected skipped disposition for record [{i}], got {other:?}"),
        }
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
    let instance_id = Target::default_instance_for(provider);
    let mut config = ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1");
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = RegisteredProvider::new(instance_id.clone(), provider, config);
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

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

fn test_request() -> Request {
    Request {
        model_id: "request-model".to_string(),
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
