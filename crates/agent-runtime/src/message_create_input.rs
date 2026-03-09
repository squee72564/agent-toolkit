use std::{collections::BTreeMap, sync::Arc};

use agent_core::{Message, Request, ResponseFormat, ToolChoice, ToolDefinition};

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

#[derive(Debug, Clone, PartialEq)]
pub struct MessageCreateInput {
    pub model: Option<String>,
    pub stream: bool,
    messages: MessagesPayload,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
    pub response_format: ResponseFormat,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub stop: Vec<String>,
    pub metadata: BTreeMap<String, String>,
}

impl MessageCreateInput {
    pub fn new(messages: Vec<Message>) -> Self {
        Self::new_owned(messages)
    }

    pub fn new_owned(messages: Vec<Message>) -> Self {
        Self {
            model: None,
            stream: false,
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

    pub fn new_shared(messages: Arc<Vec<Message>>) -> Self {
        Self {
            model: None,
            stream: false,
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

    pub fn user(text: impl Into<String>) -> Self {
        Self::from(text.into())
    }

    pub fn messages(&self) -> &[Message] {
        self.messages.as_slice()
    }

    pub fn messages_mut(&mut self) -> &mut Vec<Message> {
        self.messages.to_mut()
    }

    pub fn into_messages(self) -> Vec<Message> {
        self.messages.into_vec()
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = tool_choice;
        self
    }

    pub fn with_response_format(mut self, response_format: ResponseFormat) -> Self {
        self.response_format = response_format;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    pub fn with_max_output_tokens(mut self, max_output_tokens: u32) -> Self {
        self.max_output_tokens = Some(max_output_tokens);
        self
    }

    pub fn with_stop<I, S>(mut self, stop: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.stop = stop.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_metadata(mut self, metadata: BTreeMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn into_request_with_options(
        self,
        default_model: Option<&str>,
        allow_empty_model: bool,
    ) -> Result<Request, RuntimeError> {
        let MessageCreateInput {
            model,
            stream,
            messages,
            tools,
            tool_choice,
            response_format,
            temperature,
            top_p,
            max_output_tokens,
            stop,
            metadata,
        } = self;

        let messages = messages.into_vec();
        if messages.is_empty() {
            return Err(RuntimeError::configuration(
                "messages().create(...) requires at least one message",
            ));
        }

        let model_id = match (model, default_model) {
            (Some(model_id), _) if !model_id.trim().is_empty() => model_id,
            (_, Some(default_model)) if !default_model.trim().is_empty() => {
                default_model.to_string()
            }
            _ if allow_empty_model => String::new(),
            _ => {
                return Err(RuntimeError::configuration(
                    "no model was provided and no default model is configured",
                ));
            }
        };

        Ok(Request {
            model_id,
            stream,
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
