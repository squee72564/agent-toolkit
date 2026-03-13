use super::*;

#[test]
fn message_input_from_str_creates_user_message() {
    let input = MessageCreateInput::from("hello");
    assert_eq!(input.messages().len(), 1);
    assert_eq!(input.messages()[0].role, MessageRole::User);
}

#[test]
fn message_input_requires_at_least_one_message() {
    let error = MessageCreateInput::new(Vec::new())
        .into_task_request()
        .expect_err("empty messages should fail");
    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
}

#[test]
fn message_input_task_request_contains_only_semantic_fields() {
    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert("trace_id".to_string(), "abc123".to_string());

    let task = MessageCreateInput::from("hello")
        .with_max_output_tokens(128)
        .with_metadata(metadata.clone())
        .into_task_request()
        .expect("task request should be built");

    assert_eq!(task.messages.len(), 1);
    assert_eq!(task.max_output_tokens, Some(128));
    assert_eq!(task.metadata, metadata);
}

#[test]
fn message_create_input_from_conversation_ref_and_owned() {
    let mut conversation = Conversation::new();
    conversation.push_user_text("u1");
    conversation.push_assistant_text("a1");

    let from_ref: MessageCreateInput = (&conversation).into();
    let from_owned: MessageCreateInput = conversation.clone().into();

    assert_eq!(from_ref, from_owned);
}
