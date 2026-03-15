use std::collections::HashMap;
use std::sync::Arc;

use agent_core::ProviderKind;
use agent_core::types::Message;

use crate::ProviderInstanceId;

use super::agent_toolkit_test_fixtures::*;
use super::*;

#[tokio::test]
async fn routed_messages_fail_fast_surfaces_typed_route_planning_failure() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::anthropic_default(),
                test_provider_client(ProviderKind::Anthropic),
            ),
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client(ProviderKind::OpenRouter),
            ),
        ]),
        observer: None,
    };
    let task = MessageCreateInput::new(vec![
        Message::user_text("hello"),
        Message::system_text("late system"),
    ])
    .into_task_request()
    .expect("task request should build");

    let error = toolkit
        .messages()
        .execute(
            task,
            Route::to(
                Target::new(ProviderInstanceId::anthropic_default())
                    .with_model("claude-sonnet-4-6"),
            )
            .with_fallback(Target::new(ProviderInstanceId::openrouter_default()))
            .with_planning_rejection_policy(PlanningRejectionPolicy::FailFast),
            ExecutionOptions::default(),
        )
        .await
        .expect_err("planning rejection must stop before fallback");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
    let failure = route_planning_failure(&error);
    assert_eq!(
        failure.reason,
        RoutePlanningFailureReason::AllAttemptsRejectedDuringPlanning
    );
    assert_eq!(failure.attempts.len(), 1);
    assert_eq!(failure.attempts[0].model, "claude-sonnet-4-6");
    assert!(matches!(
        failure.attempts[0].disposition,
        AttemptDisposition::Skipped {
            reason: SkipReason::AdapterPlanningRejected { .. }
        }
    ));
}

#[tokio::test]
async fn routed_messages_create_uses_explicit_provider_instance_route() {
    let base_url = spawn_json_success_stub("req_routed_custom_instance").await;
    let toolkit = AgentToolkit::builder()
        .with_openai_instance(
            "openai-primary",
            ProviderConfig::new("primary-key")
                .with_base_url("http://127.0.0.1:1")
                .with_default_model("gpt-4.1-mini"),
        )
        .with_openai_instance(
            "openai-secondary",
            ProviderConfig::new("secondary-key")
                .with_base_url(&base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("builder should register multiple openai instances");

    let (_response, meta) = toolkit
        .messages()
        .create_with_meta(
            MessageCreateInput::user("hello"),
            Route::to(Target::new("openai-secondary").with_model("gpt-5-mini")),
        )
        .await
        .expect("request should target the named provider instance");

    assert_eq!(
        meta.selected_provider_instance,
        ProviderInstanceId::new("openai-secondary")
    );
    assert_eq!(meta.selected_provider_kind, ProviderKind::OpenAi);
}

#[tokio::test]
async fn routed_messages_emit_attempt_skipped_without_execution_events_for_planning_rejection() {
    let base_url = spawn_json_success_stub("req_routed_skip_success").await;
    let observer = Arc::new(RecordingObserver::new());
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::anthropic_default(),
                test_provider_client(ProviderKind::Anthropic),
            ),
            (
                ProviderInstanceId::openai_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenAi,
                    &base_url,
                    Some("gpt-5-mini"),
                ),
            ),
        ]),
        observer: Some(observer.clone()),
    };

    let task = MessageCreateInput::new(vec![
        Message::user_text("hello"),
        Message::system_text("late system"),
    ])
    .into_task_request()
    .expect("task request should build");

    let (_response, meta) = toolkit
        .messages()
        .execute_with_meta(
            task,
            Route::to(
                Target::new(ProviderInstanceId::anthropic_default())
                    .with_model("claude-sonnet-4-6"),
            )
            .with_fallback(
                Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5-mini"),
            )
            .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
            ExecutionOptions::default(),
        )
        .await
        .expect("route should skip rejected attempt and succeed");

    assert_eq!(
        meta.selected_provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(meta.selected_provider_kind, ProviderKind::OpenAi);

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec![
            "request_start",
            "attempt_skipped",
            "attempt_start",
            "attempt_success",
            "request_end",
        ]
    );

    let skipped = as_attempt_skipped(&events[1]);
    assert_eq!(
        skipped.provider_instance,
        ProviderInstanceId::anthropic_default()
    );
    assert_eq!(skipped.provider_kind, ProviderKind::Anthropic);
    assert_eq!(skipped.model, "claude-sonnet-4-6");
    assert_eq!(skipped.target_index, 0);
    assert_eq!(skipped.attempt_index, 0);
    assert!(skipped.elapsed >= std::time::Duration::ZERO);
    assert!(matches!(
        skipped.reason,
        SkipReason::AdapterPlanningRejected { .. }
    ));
}

#[tokio::test]
async fn routed_messages_success_preserves_failed_and_skipped_attempt_history() {
    let success_url = spawn_json_success_stub("req_routed_history_success").await;
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenRouter,
                    "http://127.0.0.1:1",
                    Some("openai/gpt-5-mini"),
                ),
            ),
            (
                ProviderInstanceId::anthropic_default(),
                test_provider_client(ProviderKind::Anthropic),
            ),
            (
                ProviderInstanceId::openai_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenAi,
                    &success_url,
                    Some("gpt-5-mini"),
                ),
            ),
        ]),
        observer: None,
    };
    let task = MessageCreateInput::new(vec![
        Message::user_text("hello"),
        Message::system_text("late system"),
    ])
    .into_task_request()
    .expect("task request should build");

    let (_response, meta) = toolkit
        .messages()
        .execute_with_meta(
            task,
            Route::to(Target::new(ProviderInstanceId::openrouter_default()))
                .with_fallback(
                    Target::new(ProviderInstanceId::anthropic_default())
                        .with_model("claude-sonnet-4-6"),
                )
                .with_fallback(
                    Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5-mini"),
                )
                .with_fallback_policy(
                    FallbackPolicy::new()
                        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Transport)),
                )
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
            ExecutionOptions::default(),
        )
        .await
        .expect("third target should succeed after a failed and skipped attempt");

    assert_eq!(
        meta.selected_provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(meta.selected_model, "gpt-5-mini");
    assert_eq!(
        meta.request_id.as_deref(),
        Some("req_routed_history_success")
    );
    assert_eq!(meta.attempts.len(), 3);
    assert_eq!(
        meta.attempts[0].provider_instance,
        ProviderInstanceId::openrouter_default()
    );
    assert!(matches!(
        meta.attempts[0].disposition,
        AttemptDisposition::Failed {
            error_kind: RuntimeErrorKind::Transport,
            ..
        }
    ));
    assert_eq!(
        meta.attempts[1].provider_instance,
        ProviderInstanceId::anthropic_default()
    );
    assert!(matches!(
        meta.attempts[1].disposition,
        AttemptDisposition::Skipped {
            reason: SkipReason::AdapterPlanningRejected { .. }
        }
    ));
    assert_eq!(
        meta.attempts[2].provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert!(matches!(
        meta.attempts[2].disposition,
        AttemptDisposition::Succeeded { .. }
    ));
}

#[tokio::test]
async fn routed_messages_terminal_error_carries_failed_and_skipped_attempt_history() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenRouter,
                    "http://127.0.0.1:1",
                    Some("openai/gpt-5-mini"),
                ),
            ),
            (
                ProviderInstanceId::anthropic_default(),
                test_provider_client(ProviderKind::Anthropic),
            ),
            (
                ProviderInstanceId::openai_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenAi,
                    "http://127.0.0.1:1",
                    Some("gpt-5-mini"),
                ),
            ),
        ]),
        observer: None,
    };
    let task = MessageCreateInput::new(vec![
        Message::user_text("hello"),
        Message::system_text("late system"),
    ])
    .into_task_request()
    .expect("task request should build");

    let error = toolkit
        .messages()
        .execute_with_meta(
            task,
            Route::to(Target::new(ProviderInstanceId::openrouter_default()))
                .with_fallback(
                    Target::new(ProviderInstanceId::anthropic_default())
                        .with_model("claude-sonnet-4-6"),
                )
                .with_fallback(
                    Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5-mini"),
                )
                .with_fallback_policy(
                    FallbackPolicy::new()
                        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Transport)),
                )
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
            ExecutionOptions::default(),
        )
        .await
        .expect_err("terminal error should carry full typed attempt history");

    let failure_meta = executed_failure_meta(&error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(failure_meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(failure_meta.selected_model, "gpt-5-mini");
    assert_eq!(failure_meta.attempts.len(), 3);
    assert_eq!(
        failure_meta.attempts[0].provider_instance,
        ProviderInstanceId::openrouter_default()
    );
    assert!(matches!(
        failure_meta.attempts[0].disposition,
        AttemptDisposition::Failed {
            error_kind: RuntimeErrorKind::Transport,
            ..
        }
    ));
    assert_eq!(
        failure_meta.attempts[1].provider_instance,
        ProviderInstanceId::anthropic_default()
    );
    assert!(matches!(
        failure_meta.attempts[1].disposition,
        AttemptDisposition::Skipped {
            reason: SkipReason::AdapterPlanningRejected { .. }
        }
    ));
    assert_eq!(
        failure_meta.attempts[2].provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert!(matches!(
        failure_meta.attempts[2].disposition,
        AttemptDisposition::Failed {
            error_kind: RuntimeErrorKind::Transport,
            ..
        }
    ));
}
