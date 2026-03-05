use std::collections::HashMap;

use agent_core::types::{ContentPart, Message, MessageRole, ToolResultContent};
use serde_json::json;

use super::*;

fn runtime_error(
    kind: RuntimeErrorKind,
    provider: Option<ProviderId>,
    status_code: Option<u16>,
    provider_code: Option<&str>,
) -> RuntimeError {
    RuntimeError {
        kind,
        message: "test error".to_string(),
        provider,
        status_code,
        request_id: None,
        provider_code: provider_code.map(ToString::to_string),
        source: None,
    }
}

#[test]
fn message_input_from_str_creates_user_message() {
    let input = MessageCreateInput::from("hello");
    assert_eq!(input.messages.len(), 1);
    assert_eq!(input.messages[0].role, MessageRole::User);
}

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
    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("toolkit should build for target resolution test");

    let options =
        SendOptions::for_target(Target::new(ProviderId::OpenRouter).with_model("openai/gpt-5"));
    let error = toolkit
        .resolve_targets(&options)
        .expect_err("unregistered provider should fail target resolution");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
}

#[test]
fn resolve_targets_deduplicates_primary_and_fallback_targets() {
    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .with_openrouter(ProviderConfig::new("test-key").with_base_url("http://127.0.0.1:1"))
        .build()
        .expect("toolkit should build for target resolution test");

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

#[test]
fn message_input_uses_default_model_when_missing() {
    let request = MessageCreateInput::from("hello")
        .into_request_with_options(Some("default-model"), false)
        .expect("default model should be used");
    assert_eq!(request.model_id, "default-model");
}

#[test]
fn message_input_allows_empty_model_for_router_path() {
    let request = MessageCreateInput::from("hello")
        .into_request_with_options(None, true)
        .expect("empty model should be allowed for router path");
    assert!(request.model_id.is_empty());
}

#[test]
fn message_input_requires_at_least_one_message() {
    let error = MessageCreateInput::new(Vec::new())
        .into_request_with_options(Some("default-model"), false)
        .expect_err("empty messages should fail");
    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
}

#[test]
fn conversation_new_is_empty() {
    let conversation = Conversation::new();
    assert!(conversation.is_empty());
    assert_eq!(conversation.len(), 0);
}

#[test]
fn conversation_with_user_text_starts_with_user_message() {
    let conversation = Conversation::with_user_text("hello");
    assert_eq!(conversation.len(), 1);
    assert_eq!(conversation.messages()[0], Message::user_text("hello"));
}

#[test]
fn conversation_push_helpers_append_expected_roles_and_parts() {
    let mut conversation = Conversation::new();
    conversation.push_user_text("user");
    conversation.push_system_text("system");
    conversation.push_assistant_text("assistant");
    conversation.push_assistant_tool_call("call_1", "search", json!({ "q": "rust" }));
    conversation.push_tool_result_json("call_1", json!({ "ok": true }));
    conversation.push_tool_result_text("call_1", "done");

    assert_eq!(conversation.len(), 6);
    assert_eq!(conversation.messages()[0], Message::user_text("user"));
    assert_eq!(conversation.messages()[1], Message::system_text("system"));
    assert_eq!(
        conversation.messages()[2],
        Message::assistant_text("assistant")
    );
    assert_eq!(
        conversation.messages()[3],
        Message::assistant_tool_call("call_1", "search", json!({ "q": "rust" }))
    );
    assert_eq!(
        conversation.messages()[4],
        Message::tool_result_json("call_1", json!({ "ok": true }))
    );
    assert_eq!(
        conversation.messages()[5],
        Message::tool_result_text("call_1", "done")
    );

    match &conversation.messages()[5].content[0] {
        ContentPart::ToolResult { tool_result } => {
            assert!(matches!(
                tool_result.content,
                ToolResultContent::Text { .. }
            ));
        }
        other => panic!("expected tool result part, got {other:?}"),
    }
}

#[test]
fn conversation_generic_push_and_extend_work() {
    let mut conversation = Conversation::new();
    conversation.push_message(Message::system_text("s1"));
    conversation.extend_messages(vec![
        Message::user_text("u1"),
        Message::assistant_text("a1"),
    ]);

    let expected = vec![
        Message::system_text("s1"),
        Message::user_text("u1"),
        Message::assistant_text("a1"),
    ];
    assert_eq!(conversation.messages(), expected.as_slice());
}

#[test]
fn conversation_to_input_and_into_input_preserve_messages() {
    let mut conversation = Conversation::with_user_text("u1");
    conversation.push_assistant_text("a1");

    let to_input = conversation.to_input();
    assert_eq!(to_input.messages, conversation.clone_messages());
    assert!(to_input.model.is_none());
    assert!(to_input.tools.is_empty());
    assert_eq!(to_input.tool_choice, ToolChoice::default());
    assert_eq!(to_input.response_format, ResponseFormat::default());
    assert!(to_input.temperature.is_none());
    assert!(to_input.top_p.is_none());
    assert!(to_input.max_output_tokens.is_none());
    assert!(to_input.stop.is_empty());
    assert!(to_input.metadata.is_empty());

    let into_input = conversation.clone().into_input();
    assert_eq!(into_input.messages, conversation.clone_messages());
    assert!(into_input.model.is_none());
    assert!(into_input.tools.is_empty());
    assert_eq!(into_input.tool_choice, ToolChoice::default());
    assert_eq!(into_input.response_format, ResponseFormat::default());
    assert!(into_input.temperature.is_none());
    assert!(into_input.top_p.is_none());
    assert!(into_input.max_output_tokens.is_none());
    assert!(into_input.stop.is_empty());
    assert!(into_input.metadata.is_empty());
}

#[test]
fn conversation_from_into_vec_roundtrip() {
    let messages = vec![Message::system_text("s1"), Message::user_text("u1")];
    let conversation = Conversation::from(messages.clone());
    let roundtrip: Vec<Message> = conversation.into();
    assert_eq!(roundtrip, messages);
}

#[test]
fn message_create_input_from_conversation_ref_and_owned() {
    let mut conversation = Conversation::with_user_text("u1");
    conversation.push_assistant_text("a1");

    let from_ref: MessageCreateInput = (&conversation).into();
    let from_owned: MessageCreateInput = conversation.clone().into();

    assert_eq!(from_ref, from_owned);
}

#[test]
fn conversation_len_is_empty_and_clear_work() {
    let mut conversation = Conversation::new();
    assert!(conversation.is_empty());

    conversation.push_user_text("u1");
    conversation.push_assistant_text("a1");
    assert_eq!(conversation.len(), 2);
    assert!(!conversation.is_empty());

    conversation.clear();
    assert_eq!(conversation.len(), 0);
    assert!(conversation.is_empty());
}

#[test]
fn conversation_messages_and_clone_messages_expose_expected_views() {
    let mut conversation = Conversation::new();
    conversation.push_user_text("u1");
    conversation.push_assistant_text("a1");

    let borrowed = conversation.messages();
    let cloned = conversation.clone_messages();

    assert_eq!(borrowed, cloned.as_slice());
    assert_eq!(borrowed[0], Message::user_text("u1"));
    assert_eq!(borrowed[1], Message::assistant_text("a1"));
}

#[derive(Debug)]
struct ObserverStub;

impl RuntimeObserver for ObserverStub {}

#[test]
fn resolve_observer_for_request_uses_expected_precedence() {
    let client_observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let toolkit_observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let send_observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);

    let resolved_send = resolve_observer_for_request(
        Some(&client_observer),
        Some(&toolkit_observer),
        Some(&send_observer),
    )
    .expect("send observer should resolve");
    assert!(std::sync::Arc::ptr_eq(resolved_send, &send_observer));

    let resolved_toolkit =
        resolve_observer_for_request(Some(&client_observer), Some(&toolkit_observer), None)
            .expect("toolkit observer should resolve");
    assert!(std::sync::Arc::ptr_eq(resolved_toolkit, &toolkit_observer));

    let resolved_client = resolve_observer_for_request(Some(&client_observer), None, None)
        .expect("client observer should resolve");
    assert!(std::sync::Arc::ptr_eq(resolved_client, &client_observer));

    let resolved_none = resolve_observer_for_request(None, None, None);
    assert!(resolved_none.is_none());
}

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
fn terminal_failure_error_returns_underlying_for_fallback_exhausted() {
    let terminal = RuntimeError {
        kind: RuntimeErrorKind::Upstream,
        message: "terminal upstream error".to_string(),
        provider: Some(ProviderId::OpenAi),
        status_code: Some(503),
        request_id: Some("req_terminal".to_string()),
        provider_code: Some("rate_limit_exceeded".to_string()),
        source: None,
    };

    let wrapped = RuntimeError::fallback_exhausted(terminal);
    let extracted = terminal_failure_error(&wrapped);

    assert_eq!(extracted.kind, RuntimeErrorKind::Upstream);
    assert_eq!(extracted.status_code, Some(503));
    assert_eq!(extracted.request_id.as_deref(), Some("req_terminal"));
}

#[test]
fn event_model_trims_and_filters_empty_values() {
    assert_eq!(
        event_model(Some("  gpt-5-mini  "), "gpt-5"),
        Some("gpt-5-mini".to_string())
    );
    assert_eq!(
        event_model(Some("  "), "  gpt-5  "),
        Some("gpt-5".to_string())
    );
    assert_eq!(event_model(None, " "), None);
}
