use std::sync::Arc;

use agent_core::ProviderInstanceId;

use crate::RuntimeErrorKind;
use crate::agent_toolkit::AgentToolkit;
use crate::agent_toolkit::tests::ObserverStub;
use crate::provider::ProviderConfig;

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
            .contains_key(&ProviderInstanceId::openai_default())
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
            .contains_key(&ProviderInstanceId::anthropic_default())
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
            .contains_key(&ProviderInstanceId::openrouter_default())
    );
}

#[test]
fn builder_registers_custom_provider_instance() {
    let toolkit = AgentToolkit::builder()
        .with_openai_instance(
            "openai-secondary",
            ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"),
        )
        .build()
        .expect("builder should register named openai instance");

    assert!(
        toolkit
            .clients
            .contains_key(&ProviderInstanceId::new("openai-secondary"))
    );
}

#[test]
fn builder_supports_multiple_instances_for_same_provider_kind() {
    let toolkit = AgentToolkit::builder()
        .with_openai_instance(
            "openai-primary",
            ProviderConfig::new("primary-key").with_base_url("http://127.0.0.1:1"),
        )
        .with_openai_instance(
            "openai-secondary",
            ProviderConfig::new("secondary-key").with_base_url("http://127.0.0.1:1"),
        )
        .build()
        .expect("builder should register multiple openai instances");

    assert!(
        toolkit
            .clients
            .contains_key(&ProviderInstanceId::new("openai-primary"))
    );
    assert!(
        toolkit
            .clients
            .contains_key(&ProviderInstanceId::new("openai-secondary"))
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
        .get(&ProviderInstanceId::openai_default())
        .expect("openai client should be registered");

    assert!(toolkit.observer.is_some());
    assert!(client.runtime.observer.is_some());
}
