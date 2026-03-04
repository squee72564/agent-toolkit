use std::collections::HashMap;

use agent_core::types::{ContentPart, Message, MessageRole, ToolResultContent};
use serde_json::json;

use super::*;

#[test]
fn message_input_from_str_creates_user_message() {
    let input = MessageCreateInput::from("hello");
    assert_eq!(input.messages.len(), 1);
    assert_eq!(input.messages[0].role, MessageRole::User);
}

#[test]
fn fallback_policy_matches_transport_or_retryable_status() {
    let policy = FallbackPolicy::default();

    let transport_error = RuntimeError {
        kind: RuntimeErrorKind::Transport,
        message: "transport".to_string(),
        provider: Some(ProviderId::OpenAi),
        status_code: None,
        request_id: None,
        provider_code: None,
        source: None,
    };
    assert!(policy.should_fallback(&transport_error));

    let rate_limit_error = RuntimeError {
        kind: RuntimeErrorKind::Upstream,
        message: "rate limited".to_string(),
        provider: Some(ProviderId::OpenAi),
        status_code: Some(429),
        request_id: None,
        provider_code: None,
        source: None,
    };
    assert!(policy.should_fallback(&rate_limit_error));
}

#[test]
fn router_requires_explicit_target_without_policy() {
    let toolkit = AgentToolkit {
        clients: HashMap::new(),
    };
    let error = toolkit
        .resolve_targets(&SendOptions::default())
        .expect_err("target resolution should fail");
    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
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
