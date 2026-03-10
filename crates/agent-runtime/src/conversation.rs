use std::sync::Arc;

use agent_core::Message;

use crate::message_create_input::MessageCreateInput;

/// Shared conversation history with copy-on-write mutation semantics.
///
/// Cloning a `Conversation` is cheap because message history is stored behind an
/// `Arc<Vec<Message>>`. Mutating methods detach only when the history is shared.
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

    /// Creates a conversation initialized with a single system message.
    pub fn with_system_text(text: impl Into<String>) -> Self {
        Self::from_messages(vec![Message::system_text(text)])
    }

    /// Returns the number of messages in the conversation.
    pub fn len(&self) -> usize {
        self.messages.as_ref().len()
    }

    /// Returns `true` when the conversation has no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.as_ref().is_empty()
    }

    /// Returns a borrowed view of the current message history.
    ///
    /// This does not clone the underlying vector.
    pub fn messages(&self) -> &[Message] {
        self.messages.as_slice()
    }

    /// Returns an owned clone of the current message history.
    ///
    /// Unlike [`Self::messages`], this always clones the underlying `Vec`.
    pub fn clone_messages(&self) -> Vec<Message> {
        self.messages.as_ref().clone()
    }

    /// Appends a message, detaching shared history only when needed.
    pub fn push_message(&mut self, message: Message) {
        Arc::make_mut(&mut self.messages).push(message);
    }

    /// Extends the conversation with additional messages.
    ///
    /// Shared history is detached only if this conversation is not uniquely
    /// owned at the time of mutation.
    pub fn extend_messages<I>(&mut self, messages: I)
    where
        I: IntoIterator<Item = Message>,
    {
        Arc::make_mut(&mut self.messages).extend(messages);
    }

    /// Appends a user text message.
    pub fn push_user_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::user_text(text));
    }

    /// Appends a system text message.
    pub fn push_system_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::system_text(text));
    }

    /// Appends an assistant text message.
    pub fn push_assistant_text(&mut self, text: impl Into<String>) {
        self.push_message(Message::assistant_text(text));
    }

    /// Appends an assistant tool-call message.
    pub fn push_assistant_tool_call(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_json: serde_json::Value,
    ) {
        self.push_message(Message::assistant_tool_call(id, name, arguments_json));
    }

    /// Appends a JSON tool result message.
    pub fn push_tool_result_json(
        &mut self,
        tool_call_id: impl Into<String>,
        value: serde_json::Value,
    ) {
        self.push_message(Message::tool_result_json(tool_call_id, value));
    }

    /// Appends a text tool result message.
    pub fn push_tool_result_text(
        &mut self,
        tool_call_id: impl Into<String>,
        text: impl Into<String>,
    ) {
        self.push_message(Message::tool_result_text(tool_call_id, text));
    }

    /// Removes all messages from the conversation.
    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.messages).clear();
    }

    /// Converts the conversation into a [`MessageCreateInput`] that shares the
    /// current message history.
    ///
    /// Later mutations to the conversation use copy-on-write and do not mutate
    /// the returned input's snapshot.
    pub fn to_input(&self) -> MessageCreateInput {
        MessageCreateInput::new_shared(Arc::clone(&self.messages))
    }

    /// Converts the conversation into a [`MessageCreateInput`], reusing the
    /// underlying message allocation when this conversation is uniquely owned.
    ///
    /// If the history is shared with other `Conversation` or input instances,
    /// the returned input keeps the shared allocation instead.
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
