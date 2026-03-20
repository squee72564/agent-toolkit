//! Ergonomic imports for common `agent_toolkit` usage.

pub use crate::runtime::{
    DirectMessagesApi, DirectStreamingApi, RoutedMessagesApi, RoutedStreamingApi,
};
pub use crate::{
    AgentToolkit, AgentToolkitBuilder, ContentPart, Conversation, Message, MessageCreateInput,
    MessageRole, Response, Route, Target, TaskRequest, ToolChoice, ToolDefinition,
};

#[cfg(feature = "anthropic")]
pub use crate::protocols::anthropic;
#[cfg(feature = "openai")]
pub use crate::protocols::openai;
#[cfg(feature = "openrouter")]
pub use crate::protocols::openrouter;
