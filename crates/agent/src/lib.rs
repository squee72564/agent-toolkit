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

/// Request models sent from the runtime to provider adapters.
pub mod request {
    pub use agent_core::types::request::*;
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
    AttemptFailureEvent, AttemptMeta, AttemptStartEvent, AttemptSuccessEvent, Conversation,
    DirectMessagesApi, DirectStreamingApi, FallbackAction, FallbackMatch, FallbackMode,
    FallbackPolicy, FallbackRule, MessageCreateInput, MessageResponseStream, MessageTextStream,
    OpenAiClient, OpenAiClientBuilder, OpenRouterClient, OpenRouterClientBuilder, ProviderConfig,
    RequestEndEvent, RequestStartEvent, ResponseMeta, RoutedMessagesApi, RoutedStreamingApi,
    RuntimeError, RuntimeErrorKind, RuntimeObserver, SendOptions, StreamCompletion, Target,
    anthropic, openai, openrouter,
};
pub use agent_transport::{
    HttpJsonResponse, HttpTransport, HttpTransportBuilder, RetryPolicy, TransportError,
};

#[cfg(test)]
mod test;
