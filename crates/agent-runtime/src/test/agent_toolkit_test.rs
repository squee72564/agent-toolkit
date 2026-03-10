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

    assert!(toolkit.clients.contains_key(&ProviderId::OpenAi));
}

#[test]
fn builder_registers_anthropic_provider() {
    let toolkit = AgentToolkit::builder()
        .with_anthropic(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("builder should register anthropic");

    assert!(toolkit.clients.contains_key(&ProviderId::Anthropic));
}

#[test]
fn builder_registers_openrouter_provider() {
    let toolkit = AgentToolkit::builder()
        .with_openrouter(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("builder should register openrouter");

    assert!(toolkit.clients.contains_key(&ProviderId::OpenRouter));
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
        .get(&ProviderId::OpenAi)
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
