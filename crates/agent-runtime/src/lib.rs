mod agent_toolkit;
mod base_client_builder;
mod clients;
mod conversation;
mod direct_messages_api;
mod fallback;
mod message_create_input;
mod observer;
mod provider_client;
mod provider_config;
mod provider_runtime;
mod routed_messages_api;
mod runtime_error;
mod send_options;
mod target;
mod types;

pub use crate::agent_toolkit::{AgentToolkit, AgentToolkitBuilder};
pub use crate::clients::{
    AnthropicClient, AnthropicClientBuilder, OpenAiClient, OpenAiClientBuilder, OpenRouterClient,
    OpenRouterClientBuilder, anthropic, openai, openrouter,
};
pub use crate::conversation::Conversation;
pub use crate::direct_messages_api::DirectMessagesApi;
pub use crate::fallback::{
    FallbackAction, FallbackMatch, FallbackMode, FallbackPolicy, FallbackRule,
};
pub use crate::message_create_input::MessageCreateInput;
pub use crate::observer::RuntimeObserver;
pub use crate::provider_config::ProviderConfig;
pub use crate::routed_messages_api::RoutedMessagesApi;
pub use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
pub use crate::send_options::SendOptions;
pub use crate::target::Target;
pub use crate::types::{
    AttemptFailureEvent, AttemptMeta, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent,
    RequestStartEvent, ResponseMeta,
};

#[cfg(test)]
mod test;
