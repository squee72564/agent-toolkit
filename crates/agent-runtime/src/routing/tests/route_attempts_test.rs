use std::collections::BTreeMap;
use std::time::Duration;

use crate::routing::{
    AttemptSpec, FallbackPolicy, PlanningRejectionPolicy, Route, Target, TransportTimeoutOverrides,
};
use agent_core::{
    FamilyOptions, NativeOptions, OpenAiCompatibleOptions, OpenRouterOptions, ProviderInstanceId,
    ProviderOptions,
};

#[test]
fn attempt_spec_builder_preserves_attempt_local_execution_state() {
    let mut extra_headers = BTreeMap::new();
    extra_headers.insert("x-attempt".to_string(), "fallback".to_string());

    let timeout_overrides = TransportTimeoutOverrides {
        request_timeout: Some(Duration::from_secs(5)),
        stream_setup_timeout: Some(Duration::from_secs(6)),
        stream_idle_timeout: Some(Duration::from_secs(7)),
    };
    let native = NativeOptions {
        family: Some(FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
            parallel_tool_calls: Some(true),
            reasoning: None,
            temperature: Some(0.2),
            top_p: None,
            max_output_tokens: Some(128),
        })),
        provider: Some(ProviderOptions::OpenRouter(Box::new(OpenRouterOptions {
            fallback_models: vec!["fallback-model".to_string()],
            ..OpenRouterOptions::default()
        }))),
    };

    let attempt = AttemptSpec::to(Target::new(ProviderInstanceId::openrouter_default()))
        .with_native_options(native.clone())
        .with_timeout_overrides(timeout_overrides.clone())
        .with_extra_headers(extra_headers.clone());

    assert_eq!(attempt.execution.native, Some(native));
    assert_eq!(attempt.execution.timeout_overrides, timeout_overrides);
    assert_eq!(attempt.execution.extra_headers, extra_headers);
}

#[test]
fn route_builder_preserves_routing_and_attempt_state() {
    let native = NativeOptions {
        family: Some(FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
            parallel_tool_calls: Some(true),
            reasoning: None,
            temperature: None,
            top_p: Some(0.8),
            max_output_tokens: None,
        })),
        provider: None,
    };

    let route = Route::to(
        AttemptSpec::to(
            Target::new(ProviderInstanceId::openai_default()).with_model("primary-model"),
        )
        .with_native_options(native.clone()),
    )
    .with_fallback(
        Target::new(ProviderInstanceId::openrouter_default()).with_model("fallback-model"),
    )
    .with_fallback_policy(FallbackPolicy::new())
    .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets);

    assert_eq!(route.primary.target.model.as_deref(), Some("primary-model"));
    assert_eq!(route.primary.execution.native, Some(native));
    assert_eq!(
        route.fallbacks[0].target.model.as_deref(),
        Some("fallback-model")
    );
    assert_eq!(
        route.planning_rejection_policy,
        PlanningRejectionPolicy::SkipRejectedTargets
    );
}
