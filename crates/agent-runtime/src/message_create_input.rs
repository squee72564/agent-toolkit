use std::{collections::BTreeMap, sync::Arc};

use agent_core::{Message, Request, ResponseFormat, TaskRequest, ToolChoice, ToolDefinition};

use crate::execution_options::{ExecutionOptions, ResponseMode};

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
/// This builder normalizes into [`TaskRequest`] plus route/execution state.
/// It keeps message storage ergonomic for direct construction and copy-on-write
/// sharing with [`Conversation`], while carrying a narrow set of legacy shim
/// fields until the older request surface is fully removed.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageCreateInput {
    /// REFACTOR-SHIM: legacy model override preserved until route construction fully migrates.
    model: Option<String>,
    /// REFACTOR-SHIM: legacy streaming flag preserved until explicit execution options replace it.
    stream: bool,
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

    /// Creates input that shares its message history behind an [`Arc`].
    ///
    /// Mutating the message list later will detach it into an owned buffer.
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

    /// REFACTOR-SHIM: legacy model override helper retained during migration to
    /// explicit route/model selection.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// REFACTOR-SHIM: legacy model override accessor.
    pub fn model_override(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// REFACTOR-SHIM: legacy streaming helper retained during migration to
    /// explicit [`ExecutionOptions`].
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// REFACTOR-SHIM: legacy streaming accessor.
    pub fn is_streaming(&self) -> bool {
        self.stream
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

    /// Converts this input into the explicit phase-1 execution boundary.
    pub fn into_task_request_parts(
        self,
    ) -> Result<(TaskRequest, Option<String>, ExecutionOptions), RuntimeError> {
        let execution = self.inferred_execution_options();
        let model = self.model.clone();
        let task = self.into_task_request()?;
        Ok((task, model, execution))
    }

    /// Infers route-wide execution options from the legacy builder shape.
    pub fn inferred_execution_options(&self) -> ExecutionOptions {
        ExecutionOptions {
            response_mode: if self.stream {
                ResponseMode::Streaming
            } else {
                ResponseMode::NonStreaming
            },
            ..ExecutionOptions::default()
        }
    }

    /// REFACTOR-SHIM: converts this input into the legacy low-level request.
    ///
    /// Prefer [`Self::into_task_request`] or
    /// [`Self::into_task_request_parts`] for new code.
    ///
    /// `default_model` is used when no explicit model override is present.
    /// When `allow_empty_model` is `true`, callers may intentionally produce a
    /// request with an empty `model_id` so routed execution can resolve the
    /// effective model from a [`crate::Target`].
    pub fn into_request_with_options(
        self,
        default_model: Option<&str>,
        allow_empty_model: bool,
    ) -> Result<Request, RuntimeError> {
        let (task, model, execution) = self.into_task_request_parts()?;

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
            stream: execution.response_mode == ResponseMode::Streaming,
            messages: task.messages,
            tools: task.tools,
            tool_choice: task.tool_choice,
            response_format: task.response_format,
            temperature: task.temperature,
            top_p: task.top_p,
            max_output_tokens: task.max_output_tokens,
            stop: task.stop,
            metadata: task.metadata,
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
