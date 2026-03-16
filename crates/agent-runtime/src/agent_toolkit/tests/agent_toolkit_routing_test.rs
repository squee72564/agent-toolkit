use std::collections::HashMap;

use agent_core::{ProviderInstanceId, ProviderKind};

use crate::agent_toolkit::AgentToolkit;
use crate::routing::{FallbackPolicy, PlanningRejectionPolicy, Route, Target};
use crate::{RuntimeErrorKind, test::test_provider_client};

#[test]
fn router_requires_explicit_target_without_policy() {
    let toolkit = AgentToolkit {
        clients: HashMap::new(),
        observer: None,
    };
    let error = toolkit
        .resolve_route_targets(&Route {
            primary: Target::new(ProviderInstanceId::openai_default()).into(),
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
            ProviderInstanceId::openai_default(),
            test_provider_client(ProviderKind::OpenAi),
        )]),
        observer: None,
    };
    let error = toolkit
        .resolve_route_targets(&Route::to(
            Target::new(ProviderInstanceId::openrouter_default()).with_model("openai/gpt-5"),
        ))
        .expect_err("unregistered provider should fail target resolution");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn resolve_route_targets_deduplicates_primary_and_fallback_targets() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::openai_default(),
                test_provider_client(ProviderKind::OpenAi),
            ),
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client(ProviderKind::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let route = Route::to(Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5"))
        .with_fallbacks(vec![
            Target::new(ProviderInstanceId::openai_default())
                .with_model("gpt-5")
                .into(),
            Target::new(ProviderInstanceId::openrouter_default())
                .with_model("openai/gpt-5")
                .into(),
            Target::new(ProviderInstanceId::openrouter_default())
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
            Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5"),
            Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5"),
            Target::new(ProviderInstanceId::openrouter_default()).with_model("openai/gpt-5"),
            Target::new(ProviderInstanceId::openrouter_default()).with_model("openai/gpt-5"),
        ]
    );
}

#[test]
fn resolve_route_targets_preserves_attempt_order() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::openai_default(),
                test_provider_client(ProviderKind::OpenAi),
            ),
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client(ProviderKind::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let route = Route::to(Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5"))
        .with_fallback(Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5"))
        .with_fallback(
            Target::new(ProviderInstanceId::openrouter_default()).with_model("openai/gpt-5"),
        );

    let targets = toolkit
        .resolve_route_targets(&route)
        .expect("route target resolution should succeed");

    assert_eq!(
        targets
            .into_iter()
            .map(|attempt| attempt.target)
            .collect::<Vec<_>>(),
        vec![
            Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5"),
            Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5"),
            Target::new(ProviderInstanceId::openrouter_default()).with_model("openai/gpt-5"),
        ]
    );
}
