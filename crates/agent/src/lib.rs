pub mod core {
    pub use agent_core::*;
}

pub mod protocols {
    pub use agent_providers::*;
}

pub mod runtime {
    pub use agent_runtime::*;
}

pub mod transport {
    pub use agent_transport::*;
}

pub mod tools {
    pub use agent_tools::*;
}

pub use agent_core::types::*;
pub use agent_runtime::{
    AgentToolkit, AgentToolkitBuilder, AnthropicClient, AnthropicClientBuilder, AttemptMeta,
    FallbackPolicy, MessageCreateInput, MessagesApi, OpenAiClient, OpenAiClientBuilder,
    OpenRouterClient, OpenRouterClientBuilder, ProviderConfig, ResponseMeta, RouterMessagesApi,
    RuntimeError, RuntimeErrorKind, SendOptions, Target, anthropic, openai, openrouter,
};
pub use agent_transport::{
    HttpJsonResponse, HttpTransport, HttpTransportBuilder, RetryPolicy, TransportError,
};

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn provider_id_reexport_matches_agent_core_type() {
        let provider_from_agent: ProviderId = ProviderId::OpenAi;
        let provider_from_core: agent_core::types::ProviderId = provider_from_agent;
        assert_eq!(provider_from_core, agent_core::types::ProviderId::OpenAi);
    }
}
