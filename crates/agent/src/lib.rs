//! Facade crate for the `agent-toolkit` workspace.
//!
//! The crate is organized as layered disclosure:
//!
//! - use the root for provider-agnostic orchestration and conversation types
//! - use [`prelude`] for ergonomic imports without recreating a fat facade
//! - use namespaced modules when you need a specific subsystem
//!
//! The main namespaces are:
//!
//! - [`core`] for shared request, response, identity, planning, and streaming types
//! - [`runtime`] for routing, observers, execution APIs, and provider configuration
//! - [`protocols`] for provider adapters and provider-specific client helpers
//! - [`transport`] for HTTP transport primitives
//! - [`tools`] for tool definitions, registries, validation, and execution
//!
//! Most consumers should start with the root exports such as [`AgentToolkit`],
//! [`Route`], [`TaskRequest`], [`MessageCreateInput`], and [`Response`], then
//! drop into [`prelude`] or a namespaced module when they need more control.

/// Core request, response, and streaming types shared across the agent workspace.
pub mod core {
    pub use agent_core::*;
}

/// Provider protocol adapters and provider-specific client helpers.
pub mod protocols {
    pub use agent_providers::*;

    #[cfg(feature = "anthropic")]
    pub use agent_runtime::{AnthropicClient, AnthropicClientBuilder, anthropic};
    #[cfg(feature = "openai")]
    pub use agent_runtime::{OpenAiClient, OpenAiClientBuilder, openai};
    #[cfg(feature = "openrouter")]
    pub use agent_runtime::{OpenRouterClient, OpenRouterClientBuilder, openrouter};
}

/// Runtime clients and routing primitives.
pub mod runtime {
    pub use agent_runtime::*;
}

/// HTTP transport primitives shared across the workspace.
pub mod transport {
    pub use agent_transport::*;
}

/// Tool definitions, registration, schema validation, and execution utilities.
pub mod tools {
    pub use agent_core::types::tool::*;
    pub use agent_tools::*;
}

/// Message roles and helper constructors for conversational inputs.
pub mod message {
    pub use agent_core::types::message::*;
}

/// Provider and transport configuration shared with HTTP adapters.
pub mod platform {
    pub use agent_core::types::platform::*;
}

/// Task and request models shared across runtime boundaries.
pub mod request {
    pub use agent_core::types::task::*;
}

/// Normalized response models returned from provider adapters.
pub mod response {
    pub use agent_core::types::response::*;
}

/// Ergonomic imports for common direct-client and routed-client flows.
pub mod prelude;

pub use agent_core::{
    ContentPart, Message, MessageRole, Response, TaskRequest, ToolCall, ToolChoice, ToolDefinition,
    ToolResult, ToolResultContent,
};
pub use agent_runtime::{
    AgentToolkit, AgentToolkitBuilder, Conversation, MessageCreateInput, Route, Target,
};

#[cfg(test)]
mod test;
