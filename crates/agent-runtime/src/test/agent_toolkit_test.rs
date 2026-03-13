use std::sync::Arc;

use super::*;

#[test]
fn builder_requires_at_least_one_provider() {
    let error = AgentToolkit::builder()
        .build()
        .expect_err("builder should require at least one provider");

    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
    assert_eq!(error.message, "at least one provider must be configured");
}

#[test]
fn builder_registers_openai_provider() {
    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("builder should register openai");

    assert!(
        toolkit
            .clients
            .contains_key(&Target::default_instance_for(ProviderId::OpenAi))
    );
}

#[test]
fn builder_registers_anthropic_provider() {
    let toolkit = AgentToolkit::builder()
        .with_anthropic(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("builder should register anthropic");

    assert!(
        toolkit
            .clients
            .contains_key(&Target::default_instance_for(ProviderId::Anthropic))
    );
}

#[test]
fn builder_registers_openrouter_provider() {
    let toolkit = AgentToolkit::builder()
        .with_openrouter(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("builder should register openrouter");

    assert!(
        toolkit
            .clients
            .contains_key(&Target::default_instance_for(ProviderId::OpenRouter))
    );
}

#[test]
fn builder_propagates_observer_to_provider_runtime() {
    let observer = Arc::new(ObserverStub);
    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .observer(observer.clone())
        .build()
        .expect("builder should register observer");

    let client = toolkit
        .clients
        .get(&Target::default_instance_for(ProviderId::OpenAi))
        .expect("openai client should be registered");

    assert!(toolkit.observer.is_some());
    assert!(client.runtime.observer.is_some());
}

#[test]
fn router_requires_explicit_target_without_policy() {
    let toolkit = AgentToolkit {
        clients: HashMap::new(),
        observer: None,
    };
    let error = toolkit
        .resolve_route_targets(&Route {
            primary: Target::new(ProviderId::OpenAi).into(),
            fallbacks: Vec::new(),
            fallback_policy: FallbackPolicy::default(),
            planning_rejection_policy: PlanningRejectionPolicy::FailFast,
        })
        .expect_err("target resolution should fail");
    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn resolve_route_targets_errors_for_unregistered_provider() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([(
            Target::default_instance_for(ProviderId::OpenAi),
            test_provider_client(ProviderId::OpenAi),
        )]),
        observer: None,
    };
    let error = toolkit
        .resolve_route_targets(&Route::to(
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
        ))
        .expect_err("unregistered provider should fail target resolution");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn resolve_route_targets_deduplicates_primary_and_fallback_targets() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(ProviderId::OpenAi),
                test_provider_client(ProviderId::OpenAi),
            ),
            (
                Target::default_instance_for(ProviderId::OpenRouter),
                test_provider_client(ProviderId::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let route = crate::Route::to(Target::new(ProviderId::OpenAi).with_model("gpt-5"))
        .with_fallbacks(vec![
            Target::new(ProviderId::OpenAi).with_model("gpt-5").into(),
            Target::new(ProviderId::OpenRouter)
                .with_model("openai/gpt-5")
                .into(),
            Target::new(ProviderId::OpenRouter)
                .with_model("openai/gpt-5")
                .into(),
        ]);

    let targets = toolkit
        .resolve_route_targets(&route)
        .expect("target resolution should succeed");

    assert_eq!(
        targets
            .into_iter()
            .map(|attempt| attempt.target)
            .collect::<Vec<_>>(),
        vec![
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
        ]
    );
}

#[test]
fn resolve_route_targets_preserves_attempt_order() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(ProviderId::OpenAi),
                test_provider_client(ProviderId::OpenAi),
            ),
            (
                Target::default_instance_for(ProviderId::OpenRouter),
                test_provider_client(ProviderId::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let route = crate::Route::to(Target::new(ProviderId::OpenAi).with_model("gpt-5"))
        .with_fallback(Target::new(ProviderId::OpenAi).with_model("gpt-5"))
        .with_fallback(Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"));

    let targets = toolkit
        .resolve_route_targets(&route)
        .expect("route target resolution should succeed");

    assert_eq!(
        targets
            .into_iter()
            .map(|attempt| attempt.target)
            .collect::<Vec<_>>(),
        vec![
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
        ]
    );
}

#[tokio::test]
async fn routed_messages_fail_fast_surfaces_typed_route_planning_failure() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(ProviderId::Anthropic),
                test_provider_client(ProviderId::Anthropic),
            ),
            (
                Target::default_instance_for(ProviderId::OpenRouter),
                test_provider_client(ProviderId::OpenRouter),
            ),
        ]),
        observer: None,
    };
    let task = agent_core::Request {
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
        metadata: std::collections::BTreeMap::new(),
    }
    .task_request();

    let error = toolkit
        .messages()
        .create_task(
            task,
            Route::to(Target::new(ProviderId::Anthropic).with_model("claude-sonnet-4-6"))
                .with_fallback(Target::new(ProviderId::OpenRouter))
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
