use agent_core::{ContentPart, Message, MessageRole, ProviderKind, Usage};

#[test]
fn root_reexports_core_types() {
    let provider = ProviderKind::OpenAi;
    assert_eq!(provider, agent_core::types::ProviderKind::OpenAi);

    let message = Message::new(MessageRole::User, vec![ContentPart::text("hello")]);
    assert_eq!(message.role, MessageRole::User);
    assert_eq!(message.content.len(), 1);
}

#[test]
fn root_and_module_types_are_interchangeable() {
    let from_root: ProviderKind = ProviderKind::Anthropic;
    let from_module: agent_core::types::ProviderKind = from_root;
    assert_eq!(from_module, agent_core::types::ProviderKind::Anthropic);
}

#[test]
fn usage_derived_total_tokens_prefers_explicit_total() {
    let usage = Usage {
        input_tokens: Some(10),
        output_tokens: Some(20),
        cached_input_tokens: None,
        total_tokens: Some(7),
    };

    assert_eq!(usage.derived_total_tokens(), 7);
}

#[test]
fn usage_derived_total_tokens_defaults_missing_values_to_zero() {
    let usage = Usage {
        input_tokens: Some(10),
        output_tokens: None,
        cached_input_tokens: None,
        total_tokens: None,
    };

    assert_eq!(usage.derived_total_tokens(), 10);
    assert_eq!(Usage::default().derived_total_tokens(), 0);
}

#[test]
fn usage_derived_total_tokens_saturates_on_overflow() {
    let usage = Usage {
        input_tokens: Some(u64::MAX),
        output_tokens: Some(1),
        cached_input_tokens: None,
        total_tokens: None,
    };

    assert_eq!(usage.derived_total_tokens(), u64::MAX);
}
