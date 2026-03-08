use std::sync::Arc;

use agent_core::Message;

use crate::message_create_input::MessageCreateInput;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    pub messages: Arc<Vec<Message>>,
}

impl Conversation {
    /// Creates an empty conversation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use agent_runtime::Conversation;
    ///
    /// let conversation = Conversation::new();
    /// assert!(conversation.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a conversation from an existing message history.
    pub fn from_messages(messages: Vec<Message>) -> Self {
        Self {
            messages: Arc::new(messages),
        }
    }

    pub fn with_system_text(text: impl Into<String>) -> Self {
        Self::from_messages(vec![Message::system_text(text)])
    }

    pub fn len(&self) -> usize {
        self.messages.as_ref().len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.as_ref().is_empty()
    }

    pub fn messages(&self) -> &[Message] {
        self.messages.as_slice()
    }

    pub fn clone_messages(&self) -> Vec<Message> {
        self.messages.as_ref().clone()
    }

    pub fn push_message(&mut self, message: Message) {
        Arc::make_mut(&mut self.messages).push(message);
    }

    pub fn extend_messages<I>(&mut self, messages: I)
    where
        I: IntoIterator<Item = Message>,
    {
        Arc::make_mut(&mut self.messages).extend(messages);
    }

    pub fn push_user_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::user_text(text));
    }

    pub fn push_system_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::system_text(text));
    }

    pub fn push_assistant_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::assistant_text(text));
    }

    pub fn push_assistant_tool_call(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_json: serde_json::Value,
    ) {
        self.push_message(Message::assistant_tool_call(id, name, arguments_json));
    }

    pub fn push_tool_result_json(
        &mut self,
        tool_call_id: impl Into<String>,
        value: serde_json::Value,
    ) {
        self.push_message(Message::tool_result_json(tool_call_id, value));
    }

    pub fn push_tool_result_text(
        &mut self,
        tool_call_id: impl Into<String>,
        text: impl Into<String>,
    ) {
        self.push_message(Message::tool_result_text(tool_call_id, text));
    }

    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.messages).clear();
    }

    pub fn to_input(&self) -> MessageCreateInput {
        MessageCreateInput::new_shared(Arc::clone(&self.messages))
    }

    pub fn into_input(self) -> MessageCreateInput {
        match Arc::try_unwrap(self.messages) {
            Ok(messages) => MessageCreateInput::new_owned(messages),
            Err(messages) => MessageCreateInput::new_shared(messages),
        }
    }
}

impl From<Vec<Message>> for Conversation {
    fn from(messages: Vec<Message>) -> Self {
        Self::from_messages(messages)
    }
}

impl From<Conversation> for Vec<Message> {
    fn from(conversation: Conversation) -> Self {
        match Arc::try_unwrap(conversation.messages) {
            Ok(messages) => messages,
            Err(messages) => messages.as_ref().clone(),
        }
    }
}
