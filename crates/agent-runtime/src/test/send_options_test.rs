use super::*;

#[test]
fn send_options_with_observer_keeps_clone_and_partial_eq_pointer_semantics() {
    let observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi)).with_observer(observer);

    let cloned = options.clone();
    assert_eq!(options, cloned);
    assert!(options.observer.is_some());

    let other_observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let different =
        SendOptions::for_target(Target::new(ProviderId::OpenAi)).with_observer(other_observer);
    assert_ne!(options, different);
}

#[test]
fn send_options_debug_redacts_observer_internals() {
    let observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi))
        .with_fallback_policy(FallbackPolicy::new(vec![Target::new(
            ProviderId::Anthropic,
        )]))
        .with_observer(observer);

    let debug = format!("{options:?}");

    assert!(debug.contains("SendOptions"));
    assert!(debug.contains("configured"));
    assert!(!debug.contains("ObserverStub"));
    assert!(debug.contains("OpenAi"));
    assert!(debug.contains("Anthropic"));
}

#[test]
fn send_options_for_target_only_sets_target() {
    let options = SendOptions::for_target(Target::new(ProviderId::OpenRouter).with_model("model"));

    assert_eq!(
        options.target,
        Some(Target::new(ProviderId::OpenRouter).with_model("model"))
    );
    assert!(options.fallback_policy.is_none());
    assert!(options.metadata.is_empty());
    assert!(options.observer.is_none());
}

#[test]
fn send_options_with_fallback_policy_preserves_metadata_and_equality() {
    let mut options = SendOptions::default();
    options
        .metadata
        .insert("trace_id".to_string(), "abc123".to_string());

    let policy = FallbackPolicy::new(vec![Target::new(ProviderId::Anthropic)])
        .with_rule(FallbackRule::retry_on_status(429));
    let updated = options.clone().with_fallback_policy(policy.clone());

    assert_eq!(
        updated.metadata.get("trace_id").map(String::as_str),
        Some("abc123")
    );
    assert_eq!(updated.fallback_policy, Some(policy.clone()));

    let mut expected = options;
    expected.fallback_policy = Some(policy);
    assert_eq!(updated, expected);
}
