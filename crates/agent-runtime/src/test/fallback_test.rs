use super::*;

fn should_retry(
    policy: &FallbackPolicy,
    error: &RuntimeError,
    provider_kind: ProviderKind,
    provider_instance: &str,
) -> bool {
    policy.should_retry_next_target(
        error,
        provider_kind,
        &crate::ProviderInstanceId::new(provider_instance),
    )
}

#[test]
fn fallback_policy_default_has_no_implicit_retry_behavior() {
    let policy = FallbackPolicy::default();

    let transport_error = runtime_error(
        RuntimeErrorKind::Transport,
        Some(ProviderId::OpenAi),
        None,
        None,
    );
    assert!(!should_retry(
        &policy,
        &transport_error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_rules_retry_on_error_kind() {
    let policy = FallbackPolicy::default()
        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Upstream));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        None,
    );
    assert!(should_retry(
        &policy,
        &error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_stop_prevents_fallback() {
    let policy =
        FallbackPolicy::default().with_rule(FallbackRule::stop_on_kind(RuntimeErrorKind::Upstream));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(!should_retry(
        &policy,
        &error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_rules_match_provider_code() {
    let policy = FallbackPolicy::default()
        .with_rule(FallbackRule::retry_on_provider_code("rate_limit_exceeded"));

    let matching_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        Some("rate_limit_exceeded"),
    );
    assert!(should_retry(
        &policy,
        &matching_error,
        ProviderKind::OpenAi,
        "openai-default"
    ));

    let non_matching_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        Some("insufficient_quota"),
    );
    assert!(!should_retry(
        &policy,
        &non_matching_error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_rules_match_provider_code_with_whitespace_normalization() {
    let policy = FallbackPolicy::default().with_rule(FallbackRule::retry_on_provider_code(
        " rate_limit_exceeded ",
    ));

    let matching_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        Some("  rate_limit_exceeded\t"),
    );
    assert!(should_retry(
        &policy,
        &matching_error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_rule_for_provider_kind_is_idempotent_for_duplicates() {
    let rule = FallbackRule::retry_on_status(429)
        .for_provider_kind(ProviderKind::OpenAi)
        .for_provider_kind(ProviderKind::OpenAi)
        .for_provider_kind(ProviderKind::OpenRouter);

    assert_eq!(rule.when.provider_kinds.len(), 2);
    assert_eq!(
        rule.when.provider_kinds,
        vec![ProviderKind::OpenAi, ProviderKind::OpenRouter]
    );
}

#[test]
fn fallback_rule_for_provider_instance_is_idempotent_for_duplicates() {
    let rule = FallbackRule::retry_on_status(429)
        .for_provider_instance(crate::ProviderInstanceId::new("openai-a"))
        .for_provider_instance(crate::ProviderInstanceId::new("openai-a"))
        .for_provider_instance(crate::ProviderInstanceId::new("openai-b"));

    assert_eq!(rule.when.provider_instances.len(), 2);
    assert_eq!(
        rule.when.provider_instances,
        vec![
            crate::ProviderInstanceId::new("openai-a"),
            crate::ProviderInstanceId::new("openai-b")
        ]
    );
}

#[test]
fn fallback_policy_rules_can_scope_to_provider_kind() {
    let policy = FallbackPolicy::default()
        .with_rule(FallbackRule::retry_on_status(429).for_provider_kind(ProviderKind::OpenRouter));

    let openrouter_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenRouter),
        Some(429),
        None,
    );
    assert!(should_retry(
        &policy,
        &openrouter_error,
        ProviderKind::OpenRouter,
        "openrouter-default"
    ));

    let openai_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(!should_retry(
        &policy,
        &openai_error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_rules_can_scope_to_provider_instance() {
    let policy = FallbackPolicy::default().with_rule(
        FallbackRule::retry_on_status(429)
            .for_provider_instance(crate::ProviderInstanceId::new("openai-secondary")),
    );

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(should_retry(
        &policy,
        &error,
        ProviderKind::OpenAi,
        "openai-secondary"
    ));
    assert!(!should_retry(
        &policy,
        &error,
        ProviderKind::OpenAi,
        "openai-primary"
    ));
}

#[test]
fn fallback_policy_rules_use_first_match_precedence() {
    let policy = FallbackPolicy::default()
        .with_rule(FallbackRule::stop_on_kind(RuntimeErrorKind::Upstream))
        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Upstream));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(!should_retry(
        &policy,
        &error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_no_match_does_not_fallback() {
    let policy = FallbackPolicy::default().with_rule(FallbackRule::retry_on_status(500));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(!should_retry(
        &policy,
        &error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_rule_requires_all_match_conditions() {
    let policy = FallbackPolicy::default().with_rule(FallbackRule {
        when: FallbackMatch {
            error_kinds: vec![RuntimeErrorKind::Upstream],
            status_codes: vec![429],
            provider_codes: vec!["rate_limit_exceeded".to_string()],
            provider_kinds: vec![ProviderKind::OpenAi],
            provider_instances: vec![crate::ProviderInstanceId::new("openai-default")],
        },
        action: FallbackAction::RetryNextTarget,
    });

    let matching_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        Some("rate_limit_exceeded"),
    );
    assert!(should_retry(
        &policy,
        &matching_error,
        ProviderKind::OpenAi,
        "openai-default"
    ));

    let wrong_status_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(503),
        Some("rate_limit_exceeded"),
    );
    assert!(!should_retry(
        &policy,
        &wrong_status_error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_provider_code_rule_does_not_match_blank_rule_value() {
    let policy = FallbackPolicy::default().with_rule(FallbackRule::retry_on_provider_code(" \t "));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        Some("rate_limit_exceeded"),
    );
    assert!(!should_retry(
        &policy,
        &error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}

#[test]
fn fallback_policy_status_rule_does_not_match_without_status_code() {
    let policy = FallbackPolicy::default().with_rule(FallbackRule::retry_on_status(429));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        None,
    );
    assert!(!should_retry(
        &policy,
        &error,
        ProviderKind::OpenAi,
        "openai-default"
    ));
}
