use reqwest::header::HeaderName;

use agent_core::{ProviderInstanceId, ProviderKind};
use agent_providers::adapter::adapter_for;

use crate::clients::BaseClientBuilder;
use crate::provider::{ProviderConfig, RegisteredProvider};

#[test]
fn registered_provider_platform_config_uses_instance_request_id_override() {
    let registered = RegisteredProvider::new(
        ProviderInstanceId::openai_default(),
        ProviderKind::OpenAi,
        ProviderConfig::new("test-key")
            .with_base_url("https://api.openai.com")
            .with_request_id_header(HeaderName::from_static("x-instance-request-id")),
    );

    let platform = registered
        .platform_config(adapter_for(ProviderKind::OpenAi).descriptor())
        .expect("platform config should build");

    assert_eq!(
        platform.request_id_header,
        HeaderName::from_static("x-instance-request-id")
    );
}

#[test]
fn registered_provider_platform_config_supports_generic_openai_compatible_kind() {
    let registered = RegisteredProvider::new(
        ProviderInstanceId::generic_openai_compatible_default(),
        ProviderKind::GenericOpenAiCompatible,
        ProviderConfig::new("test-key").with_base_url("https://example.test/v1"),
    );

    let descriptor = adapter_for(ProviderKind::GenericOpenAiCompatible).descriptor();
    let platform = registered
        .platform_config(descriptor)
        .expect("generic openai-compatible platform config should build");

    assert_eq!(platform.protocol, agent_core::ProtocolKind::OpenAI);
    assert_eq!(platform.base_url, "https://example.test/v1");
}

#[test]
fn base_client_builder_preserves_request_id_header_from_provider_config() {
    let builder = BaseClientBuilder::from_provider_config(
        ProviderConfig::new("test-key")
            .with_base_url("https://api.openai.com")
            .with_request_id_header(HeaderName::from_static("x-instance-request-id")),
    );

    assert_eq!(
        builder.request_id_header,
        Some(HeaderName::from_static("x-instance-request-id"))
    );
}
