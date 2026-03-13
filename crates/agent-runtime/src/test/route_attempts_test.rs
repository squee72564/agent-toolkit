use std::collections::BTreeMap;
use std::time::Duration;

use agent_core::{FamilyOptions, NativeOptions, OpenAiCompatibleOptions, ProviderOptions};

use crate::routed_messages_api::apply_model_override;
use crate::{
    AttemptSpec, FallbackPolicy, OpenRouterOptions, PlanningRejectionPolicy, Route, Target,
    TransportTimeoutOverrides,
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
        })),
        provider: Some(ProviderOptions::OpenRouter(Box::new(
            OpenRouterOptions::new().with_route("fallback"),
        ))),
    };

    let attempt = AttemptSpec::to(Target::new(agent_core::ProviderId::OpenRouter))
        .with_native_options(native.clone())
        .with_timeout_overrides(timeout_overrides.clone())
        .with_extra_headers(extra_headers.clone());

    assert_eq!(attempt.execution.native, Some(native));
    assert_eq!(attempt.execution.timeout_overrides, timeout_overrides);
    assert_eq!(attempt.execution.extra_headers, extra_headers);
}

#[test]
fn apply_model_override_updates_only_primary_target_and_preserves_route_state() {
    let native = NativeOptions {
        family: Some(FamilyOptions::OpenAiCompatible(OpenAiCompatibleOptions {
            parallel_tool_calls: Some(true),
            reasoning: None,
        })),
        provider: None,
    };

    let route = Route::to(
        AttemptSpec::to(Target::new(agent_core::ProviderId::OpenAi))
            .with_native_options(native.clone()),
    )
    .with_fallback(Target::new(agent_core::ProviderId::OpenRouter).with_model("fallback-model"))
    .with_fallback_policy(FallbackPolicy::new())
    .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets);

    let overridden = apply_model_override(route, Some("primary-model".to_string()));

    assert_eq!(
        overridden.primary.target.model.as_deref(),
        Some("primary-model")
    );
    assert_eq!(overridden.primary.execution.native, Some(native));
    assert_eq!(
        overridden.fallbacks[0].target.model.as_deref(),
        Some("fallback-model")
    );
    assert_eq!(
        overridden.planning_rejection_policy,
        PlanningRejectionPolicy::SkipRejectedTargets
    );
}
