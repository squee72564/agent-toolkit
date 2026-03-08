use super::*;

#[test]
fn router_requires_explicit_target_without_policy() {
    let toolkit = AgentToolkit {
        clients: HashMap::new(),
        observer: None,
    };
    let error = toolkit
        .resolve_targets(&SendOptions::default())
        .expect_err("target resolution should fail");
    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn fallback_policy_requires_targets_without_primary_target() {
    let toolkit = AgentToolkit {
        clients: HashMap::new(),
        observer: None,
    };
    let options = SendOptions::default().with_fallback_policy(FallbackPolicy::new(vec![]));
    let error = toolkit
        .resolve_targets(&options)
        .expect_err("empty fallback target list should fail without primary target");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn resolve_targets_errors_for_unregistered_provider() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([(ProviderId::OpenAi, test_provider_client(ProviderId::OpenAi))]),
        observer: None,
    };

    let options =
        SendOptions::for_target(Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"));
    let error = toolkit
        .resolve_targets(&options)
        .expect_err("unregistered provider should fail target resolution");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn resolve_targets_deduplicates_primary_and_fallback_targets() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (ProviderId::OpenAi, test_provider_client(ProviderId::OpenAi)),
            (
                ProviderId::OpenRouter,
                test_provider_client(ProviderId::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi).with_model("gpt-5"))
        .with_fallback_policy(FallbackPolicy::new(vec![
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
        ]));

    let targets = toolkit
        .resolve_targets(&options)
        .expect("target resolution should succeed");

    assert_eq!(
        targets,
        vec![
            Target::new(ProviderId::OpenAi).with_model("gpt-5"),
            Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"),
        ]
    );
}
