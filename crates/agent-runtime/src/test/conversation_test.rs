use super::*;

#[test]
fn conversation_new_is_empty() {
    let conversation = Conversation::new();
    assert!(conversation.is_empty());
    assert_eq!(conversation.len(), 0);
}

#[test]
fn conversation_with_system_text_starts_with_system_text() {
    let conversation = Conversation::with_system_text("You are a helpful assistant");
    assert_eq!(conversation.len(), 1);
    assert_eq!(
        conversation.messages()[0],
        Message::system_text("You are a helpful assistant")
    );
}

#[test]
fn conversation_with_user_text_starts_with_user_message() {
    let mut conversation = Conversation::new();
    conversation.push_user_text("hello");
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
    let mut conversation = Conversation::new();
    conversation.push_user_text("u1");
    conversation.push_assistant_text("a1");

    let to_input = conversation.to_input();
    assert_eq!(to_input.messages(), conversation.messages());
    assert!(to_input.tools.is_empty());
    assert_eq!(to_input.tool_choice, ToolChoice::default());
    assert_eq!(to_input.response_format, ResponseFormat::default());
    assert!(to_input.temperature.is_none());
    assert!(to_input.top_p.is_none());
    assert!(to_input.max_output_tokens.is_none());
    assert!(to_input.stop.is_empty());
    assert!(to_input.metadata.is_empty());

    let into_input = conversation.clone().into_input();
    assert_eq!(into_input.messages(), conversation.messages());
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
fn conversation_into_vec_reuses_allocation_when_unique() {
    let mut conversation = Conversation::new();
    conversation.push_user_text("u1");
    conversation.push_assistant_text("a1");

    let original_ptr = conversation.messages.as_ptr();
    let messages: Vec<Message> = conversation.into();

    assert_eq!(messages.as_ptr(), original_ptr);
}

#[test]
fn conversation_into_vec_clones_when_messages_are_shared() {
    let mut conversation = Conversation::new();
    conversation.push_user_text("u1");
    conversation.push_assistant_text("a1");
    let shared = conversation.clone();

    let original_ptr = shared.messages.as_ptr();
    let messages: Vec<Message> = conversation.into();

    assert_ne!(messages.as_ptr(), original_ptr);
    assert_eq!(messages.as_slice(), shared.messages());
}

#[test]
fn conversation_to_input_uses_copy_on_write_for_later_mutation() {
    let mut conversation = Conversation::new();
    conversation.push_user_text("u1");
    let first_input = conversation.to_input();

    conversation.push_assistant_text("a1");
    let second_input = conversation.to_input();

    assert_eq!(first_input.messages(), &[Message::user_text("u1")]);
    assert_eq!(
        second_input.messages(),
        &[Message::user_text("u1"), Message::assistant_text("a1")]
    );
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
