use super::*;

#[test]
fn message_input_from_str_creates_user_message() {
    let input = MessageCreateInput::from("hello");
    assert_eq!(input.messages().len(), 1);
    assert_eq!(input.messages()[0].role, MessageRole::User);
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
fn message_create_input_from_conversation_ref_and_owned() {
    let mut conversation = Conversation::new();
    conversation.push_user_text("u1");
    conversation.push_assistant_text("a1");

    let from_ref: MessageCreateInput = (&conversation).into();
    let from_owned: MessageCreateInput = conversation.clone().into();

    assert_eq!(from_ref, from_owned);
}
