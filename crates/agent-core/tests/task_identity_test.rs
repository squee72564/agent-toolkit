use agent_core::{ProviderFamilyId, ProviderInstanceId, ProviderKind};

#[test]
fn provider_kind_and_instance_identity_are_distinct() {
    assert_eq!(ProviderKind::OpenAi, agent_core::ProviderKind::OpenAi);
    assert_eq!(
        ProviderInstanceId::generic_openai_compatible_default().as_str(),
        "generic-openai-compatible-default"
    );
    assert_eq!(
        ProviderFamilyId::OpenAiCompatible,
        ProviderFamilyId::OpenAiCompatible
    );
}
