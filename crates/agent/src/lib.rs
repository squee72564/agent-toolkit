/// Core request, response, and streaming types shared across the agent workspace.
pub mod core {
    pub use agent_core::*;
}

/// Provider protocol adapters and translation primitives.
pub mod protocols {
    pub use agent_providers::*;
}

/// Runtime clients and routing primitives.
pub mod runtime {
    pub use agent_runtime::*;
}

/// HTTP transport primitives shared across the workspace.
pub mod transport {
    pub use agent_transport::*;
}

/// Tool registration, schema validation, and execution utilities.
pub mod tools {
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

/// Tool definitions and mixed-content message parts.
pub mod tool {
    pub use agent_core::types::tool::*;
}

pub use agent_core::types::*;
pub use agent_runtime::{
    AgentToolkit, AgentToolkitBuilder, AnthropicClient, AnthropicClientBuilder,
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, Conversation, DirectMessagesApi,
    DirectStreamingApi, ExecutionOptions, FallbackAction, FallbackMatch, FallbackPolicy,
    FallbackRule, MessageCreateInput, MessageResponseStream, MessageTextStream, OpenAiClient,
    OpenAiClientBuilder, OpenRouterClient, OpenRouterClientBuilder, ProviderConfig,
    RequestEndEvent, RequestStartEvent, ResponseMeta, ResponseMode, Route, RoutedMessagesApi,
    RoutedStreamingApi, RuntimeError, RuntimeErrorKind, RuntimeObserver, StreamCompletion, Target,
    TransportOptions, anthropic, openai, openrouter,
};
pub use agent_transport::{
    HttpJsonResponse, HttpTransport, HttpTransportBuilder, RetryPolicy, TransportError,
};

#[cfg(test)]
mod test;
