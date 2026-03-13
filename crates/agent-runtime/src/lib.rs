//! Runtime clients and routing primitives for `agent-toolkit`.
//!
//! This crate exposes two main entry points:
//!
//! - Direct provider clients such as [`OpenAiClient`] for single-provider use.
//! - [`AgentToolkit`] for routed execution across multiple configured providers
//!   with target selection, fallback, and observer hooks.
//!
//! The explicit low-level execution boundary is
//! [`agent_core::TaskRequest`] + [`Route`] + [`ExecutionOptions`].
//!
//! Use `messages()` for non-streaming requests and `streaming()` when you need
//! canonical stream envelopes or text deltas.

mod agent_toolkit;
mod attempt_execution_options;
mod attempt_spec;
mod base_client_builder;
mod clients;
mod conversation;
mod direct_messages_api;
mod direct_streaming_api;
mod execution_options;
mod fallback;
mod message_create_input;
mod message_response_stream;
mod message_text_stream;
mod observer;
mod planning_rejection_policy;
mod provider_client;
mod provider_config;
mod provider_runtime;
mod provider_stream_runtime;
mod registered_provider;
mod route;
mod route_planning;
mod routed_messages_api;
mod routed_streaming_api;
mod runtime_error;
mod target;
mod types;

pub use crate::agent_toolkit::{AgentToolkit, AgentToolkitBuilder};
pub use crate::attempt_execution_options::{AttemptExecutionOptions, TransportTimeoutOverrides};
pub use crate::attempt_spec::AttemptSpec;
pub use crate::clients::{
    AnthropicClient, AnthropicClientBuilder, OpenAiClient, OpenAiClientBuilder, OpenRouterClient,
    OpenRouterClientBuilder, anthropic, openai, openrouter,
};
pub use crate::conversation::Conversation;
pub use crate::direct_messages_api::DirectMessagesApi;
pub use crate::direct_streaming_api::DirectStreamingApi;
pub use crate::execution_options::{ExecutionOptions, ResponseMode, TransportOptions};
pub use crate::fallback::{
    FallbackAction, FallbackMatch, FallbackMode, FallbackPolicy, FallbackRule,
};
pub use crate::message_create_input::MessageCreateInput;
pub use crate::message_response_stream::{MessageResponseStream, StreamCompletion};
pub use crate::message_text_stream::MessageTextStream;
pub use crate::observer::RuntimeObserver;
pub use crate::planning_rejection_policy::PlanningRejectionPolicy;
pub use crate::provider_config::ProviderConfig;
pub use crate::registered_provider::RegisteredProvider;
pub use crate::route::Route;
pub use crate::routed_messages_api::RoutedMessagesApi;
pub use crate::routed_streaming_api::RoutedStreamingApi;
pub use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
pub use crate::target::Target;
pub use crate::types::{
    AttemptFailureEvent, AttemptMeta, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent,
    RequestStartEvent, ResponseMeta,
};
pub use agent_core::{
    AnthropicFamilyOptions, AnthropicOptions, FamilyOptions, NativeOptions,
    OpenAiCompatibleOptions, OpenAiOptions, OpenRouterOptions, ProviderOptions,
};

#[cfg(test)]
mod test;
