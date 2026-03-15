#![allow(clippy::result_large_err)]

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
mod attempt;
mod clients;
mod conversation;
mod api;
mod execution_options;
mod fallback;
mod message_create_input;
mod message_response_stream;
mod message_text_stream;
mod observer;
mod planner;
mod provider;
mod provider_runtime;
mod provider_stream_runtime;
mod route;
mod runtime_error;
mod target;
mod types;

pub use crate::agent_toolkit::{AgentToolkit, AgentToolkitBuilder};
pub use crate::attempt::{AttemptExecutionOptions, AttemptSpec, TransportTimeoutOverrides};
pub use crate::clients::{
    AnthropicClient, AnthropicClientBuilder, OpenAiClient, OpenAiClientBuilder, OpenRouterClient,
    OpenRouterClientBuilder, anthropic, openai, openrouter,
};
pub use crate::conversation::Conversation;
pub use crate::api::{DirectMessagesApi, DirectStreamingApi, RoutedMessagesApi, RoutedStreamingApi};
pub use crate::execution_options::{ExecutionOptions, ResponseMode, TransportOptions};
pub use crate::fallback::{FallbackAction, FallbackMatch, FallbackPolicy, FallbackRule};
pub use crate::message_create_input::MessageCreateInput;
pub use crate::message_response_stream::{MessageResponseStream, StreamCompletion};
pub use crate::message_text_stream::MessageTextStream;
pub use crate::observer::RuntimeObserver;
pub use crate::planner::PlanningRejectionPolicy;
pub use crate::provider::{ProviderConfig, RegisteredProvider};
pub use crate::route::Route;
pub use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
pub use crate::target::Target;
pub use crate::types::{
    ExecutedFailureMeta, ResponseMeta, RoutePlanningFailure, RoutePlanningFailureReason,
};

pub use crate::attempt::{AttemptDisposition, AttemptRecord, SkipReason};

pub use crate::observer::{
    AttemptFailureEvent, AttemptSkippedEvent, AttemptStartEvent, AttemptSuccessEvent,
    RequestEndEvent, RequestStartEvent,
};

pub use agent_core::{
    AnthropicFamilyOptions, AnthropicOptions, FamilyOptions, NativeOptions,
    OpenAiCompatibleOptions, OpenAiOptions, OpenRouterOptions, ProviderInstanceId, ProviderKind,
    ProviderOptions,
};

#[cfg(test)]
mod test;
