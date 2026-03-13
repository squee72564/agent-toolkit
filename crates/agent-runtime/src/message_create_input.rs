use std::{collections::BTreeMap, sync::Arc};

use agent_core::{Message, ResponseFormat, TaskRequest, ToolChoice, ToolDefinition};

use crate::conversation::Conversation;
use crate::runtime_error::RuntimeError;

#[derive(Debug, Clone, PartialEq)]
enum MessagesPayload {
    Owned(Vec<Message>),
    Shared(Arc<Vec<Message>>),
}

impl Default for MessagesPayload {
    fn default() -> Self {
        Self::Owned(Vec::new())
    }
}

impl MessagesPayload {
    fn as_slice(&self) -> &[Message] {
        match self {
            Self::Owned(messages) => messages.as_slice(),
            Self::Shared(messages) => messages.as_slice(),
        }
    }

    fn into_vec(self) -> Vec<Message> {
        match self {
            Self::Owned(messages) => messages,
            Self::Shared(messages) => messages.as_ref().clone(),
        }
    }

    fn to_mut(&mut self) -> &mut Vec<Message> {
        if let Self::Shared(messages) = self {
            let cloned = messages.as_ref().clone();
            *self = Self::Owned(cloned);
        }

        match self {
            Self::Owned(messages) => messages,
            Self::Shared(_) => unreachable!("shared payload should materialize before mutation"),
        }
    }
}

/// High-level task input used by the `messages` and `streaming` APIs.
///
/// This builder normalizes into [`TaskRequest`] only.
/// It keeps message storage ergonomic for direct construction and copy-on-write
/// sharing with [`Conversation`].
#[derive(Debug, Clone, PartialEq)]
pub struct MessageCreateInput {
    messages: MessagesPayload,
    /// Tool definitions exposed to the model for this request.
    pub tools: Vec<ToolDefinition>,
    /// Tool selection policy for the request.
    pub tool_choice: ToolChoice,
    /// Requested response format.
    pub response_format: ResponseFormat,
    /// Sampling temperature override.
    pub temperature: Option<f32>,
    /// Nucleus sampling override.
    pub top_p: Option<f32>,
    /// Maximum number of output tokens to generate.
    pub max_output_tokens: Option<u32>,
    /// Stop sequences for generation.
    pub stop: Vec<String>,
    /// Opaque request metadata forwarded to the provider adapter.
    pub metadata: BTreeMap<String, String>,
}

impl MessageCreateInput {
    /// Creates input from an owned message history.
    pub fn new(messages: Vec<Message>) -> Self {
        Self::new_owned(messages)
    }

    /// Creates input from an owned message history.
    pub fn new_owned(messages: Vec<Message>) -> Self {
        Self {
            messages: MessagesPayload::Owned(messages),
            tools: Vec::new(),
            tool_choice: ToolChoice::default(),
            response_format: ResponseFormat::default(),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            stop: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Creates input that shares its message history behind an [`Arc`].
    ///
    /// Mutating the message list later will detach it into an owned buffer.
    pub fn new_shared(messages: Arc<Vec<Message>>) -> Self {
        Self {
            messages: MessagesPayload::Shared(messages),
            tools: Vec::new(),
            tool_choice: ToolChoice::default(),
            response_format: ResponseFormat::default(),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            stop: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Creates input containing a single user text message.
    pub fn user(text: impl Into<String>) -> Self {
        Self::from(text.into())
    }

    /// Returns the current message slice.
    pub fn messages(&self) -> &[Message] {
        self.messages.as_slice()
    }

    /// Returns a mutable message vector, materializing shared history if
    /// needed.
    pub fn messages_mut(&mut self) -> &mut Vec<Message> {
        self.messages.to_mut()
    }

    /// Consumes the input and returns an owned message vector.
    pub fn into_messages(self) -> Vec<Message> {
        self.messages.into_vec()
    }

    /// Replaces the tool definitions for this request.
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Sets the tool selection policy.
    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = tool_choice;
        self
    }

    /// Sets the response format.
    pub fn with_response_format(mut self, response_format: ResponseFormat) -> Self {
        self.response_format = response_format;
        self
    }

    /// Sets the sampling temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Sets the nucleus sampling parameter.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Sets the maximum output token count.
    pub fn with_max_output_tokens(mut self, max_output_tokens: u32) -> Self {
        self.max_output_tokens = Some(max_output_tokens);
        self
    }

    /// Replaces the stop sequences.
    pub fn with_stop<I, S>(mut self, stop: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.stop = stop.into_iter().map(Into::into).collect();
        self
    }

    /// Replaces the request metadata map.
    pub fn with_metadata(mut self, metadata: BTreeMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Converts this input into a semantic task request.
    pub fn into_task_request(self) -> Result<TaskRequest, RuntimeError> {
        let MessageCreateInput {
            messages,
            tools,
            tool_choice,
            response_format,
            temperature,
            top_p,
            max_output_tokens,
            stop,
            metadata,
            ..
        } = self;

        let messages = messages.into_vec();
        if messages.is_empty() {
            return Err(RuntimeError::configuration(
                "messages().create(...) requires at least one message",
            ));
        }

        Ok(TaskRequest {
            messages,
            tools,
            tool_choice,
            response_format,
            temperature,
            top_p,
            max_output_tokens,
            stop,
            metadata,
        })
    }
}

impl From<String> for MessageCreateInput {
    fn from(text: String) -> Self {
        Self::new(vec![Message::user_text(text)])
    }
}

impl From<&str> for MessageCreateInput {
    fn from(text: &str) -> Self {
        Self::from(text.to_string())
    }
}

impl From<Vec<Message>> for MessageCreateInput {
    fn from(messages: Vec<Message>) -> Self {
        Self::new(messages)
    }
}

impl From<Conversation> for MessageCreateInput {
    fn from(conversation: Conversation) -> Self {
        conversation.into_input()
    }
}

impl From<&Conversation> for MessageCreateInput {
    fn from(conversation: &Conversation) -> Self {
        conversation.to_input()
    }
}
