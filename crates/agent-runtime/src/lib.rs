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
mod api;
mod clients;
mod execution_options;
mod message;
mod message_response_stream;
mod observability;
mod provider;
mod provider_runtime;
mod provider_stream_runtime;
mod routing;
mod runtime_error;
mod types;

pub use crate::agent_toolkit::{AgentToolkit, AgentToolkitBuilder};
pub use crate::api::{
    DirectMessagesApi, DirectStreamingApi, RoutedMessagesApi, RoutedStreamingApi,
};
#[cfg(feature = "anthropic")]
pub use crate::clients::{AnthropicClient, AnthropicClientBuilder, anthropic};
#[cfg(feature = "openai")]
pub use crate::clients::{OpenAiClient, OpenAiClientBuilder, openai};
#[cfg(feature = "openrouter")]
pub use crate::clients::{OpenRouterClient, OpenRouterClientBuilder, openrouter};
pub use crate::execution_options::{ExecutionOptions, ResponseMode, TransportOptions};
pub use crate::message::{Conversation, MessageCreateInput, MessageTextStream};
pub use crate::message_response_stream::{MessageResponseStream, StreamCompletion};
pub use crate::observability::RuntimeObserver;
pub use crate::provider::{ProviderConfig, RegisteredProvider};
pub use crate::routing::{
    AttemptDisposition, AttemptExecutionOptions, AttemptRecord, AttemptSpec, FallbackAction,
    FallbackMatch, FallbackPolicy, FallbackRule, PlanningRejectionPolicy, Route,
    RoutePlanningFailure, RoutePlanningFailureReason, SkipReason, Target,
    TransportTimeoutOverrides,
};
pub use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
pub use crate::types::{ExecutedFailureMeta, ResponseMeta};

pub use crate::observability::{
    AttemptFailureEvent, AttemptSkippedEvent, AttemptStartEvent, AttemptSuccessEvent,
    RequestEndEvent, RequestStartEvent,
};

#[cfg(test)]
mod test;
