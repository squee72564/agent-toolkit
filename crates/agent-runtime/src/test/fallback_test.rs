use super::*;

#[test]
fn fallback_policy_matches_transport_or_retryable_status() {
    let policy = FallbackPolicy::default();

    let transport_error = runtime_error(
        RuntimeErrorKind::Transport,
        Some(ProviderId::OpenAi),
        None,
        None,
    );
    assert!(policy.should_fallback(&transport_error));

    let rate_limit_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(policy.should_fallback(&rate_limit_error));
}

#[test]
fn fallback_policy_rules_only_retry_on_error_kind() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Upstream));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        None,
    );
    assert!(policy.should_fallback(&error));
}

#[test]
fn fallback_policy_rules_only_stop_prevents_fallback() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::stop_on_kind(RuntimeErrorKind::Upstream));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(!policy.should_fallback(&error));
}

#[test]
fn fallback_policy_rules_match_provider_code() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_provider_code("rate_limit_exceeded"));

    let matching_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        Some("rate_limit_exceeded"),
    );
    assert!(policy.should_fallback(&matching_error));

    let non_matching_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        Some("insufficient_quota"),
    );
    assert!(!policy.should_fallback(&non_matching_error));
}

#[test]
fn fallback_policy_rules_match_provider_code_with_whitespace_normalization() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_provider_code(
            " rate_limit_exceeded ",
        ));

    let matching_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        None,
        Some("  rate_limit_exceeded\t"),
    );
    assert!(policy.should_fallback(&matching_error));
}

#[test]
fn fallback_rule_for_provider_is_idempotent_for_duplicates() {
    let rule = FallbackRule::retry_on_status(429)
        .for_provider(ProviderId::OpenAi)
        .for_provider(ProviderId::OpenAi)
        .for_provider(ProviderId::OpenRouter);

    assert_eq!(rule.when.providers.len(), 2);
    assert_eq!(
        rule.when.providers,
        vec![ProviderId::OpenAi, ProviderId::OpenRouter]
    );
}

#[test]
fn fallback_policy_rules_can_scope_to_provider() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_status(429).for_provider(ProviderId::OpenRouter));

    let openrouter_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenRouter),
        Some(429),
        None,
    );
    assert!(policy.should_fallback(&openrouter_error));

    let openai_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(!policy.should_fallback(&openai_error));
}

#[test]
fn fallback_policy_rules_use_first_match_precedence() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::stop_on_kind(RuntimeErrorKind::Upstream))
        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Upstream));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(!policy.should_fallback(&error));
}

#[test]
fn fallback_policy_legacy_only_ignores_rules() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::LegacyOnly)
        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Validation));

    let error = runtime_error(
        RuntimeErrorKind::Validation,
        Some(ProviderId::OpenAi),
        None,
        None,
    );
    assert!(!policy.should_fallback(&error));
}

#[test]
fn fallback_policy_legacy_or_rules_applies_rule_when_legacy_does_not() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::LegacyOrRules)
        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Validation));

    let error = runtime_error(
        RuntimeErrorKind::Validation,
        Some(ProviderId::OpenAi),
        None,
        None,
    );
    assert!(policy.should_fallback(&error));
}

#[test]
fn fallback_policy_rules_only_no_match_does_not_fallback() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_status(500));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        None,
    );
    assert!(!policy.should_fallback(&error));
}

#[test]
fn fallback_policy_rule_requires_all_match_conditions() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule {
            when: FallbackMatch {
                error_kinds: vec![RuntimeErrorKind::Upstream],
                status_codes: vec![429],
                provider_codes: vec!["rate_limit_exceeded".to_string()],
                providers: vec![ProviderId::OpenAi],
            },
            action: FallbackAction::RetryNextTarget,
        });

    let matching_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        Some("rate_limit_exceeded"),
    );
    assert!(policy.should_fallback(&matching_error));

    let wrong_status_error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(503),
        Some("rate_limit_exceeded"),
    );
    assert!(!policy.should_fallback(&wrong_status_error));
}

#[test]
fn fallback_policy_provider_code_rule_does_not_match_blank_rule_value() {
    let policy = FallbackPolicy::default()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_provider_code(" \t "));

    let error = runtime_error(
        RuntimeErrorKind::Upstream,
        Some(ProviderId::OpenAi),
        Some(429),
        Some("rate_limit_exceeded"),
    );
    assert!(!policy.should_fallback(&error));
}
