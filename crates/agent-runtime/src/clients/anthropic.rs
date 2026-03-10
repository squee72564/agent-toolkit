use crate::base_client_builder::BaseClientBuilder;
use crate::clients::common::{ClientEnv, impl_provider_client};
use crate::provider_client::ProviderClient;
use agent_core::ProviderId;

const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const ANTHROPIC_BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
const ANTHROPIC_MODEL_ENV: &str = "ANTHROPIC_MODEL";

/// Direct client for Anthropic requests.
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    inner: ProviderClient,
}

/// Builder for [`AnthropicClient`].
#[derive(Debug, Clone, Default)]
pub struct AnthropicClientBuilder {
    inner: BaseClientBuilder,
}

impl_provider_client!(
    client = AnthropicClient,
    builder = AnthropicClientBuilder,
    provider = ProviderId::Anthropic,
    constructor = anthropic,
    env = ClientEnv::new(
        ANTHROPIC_API_KEY_ENV,
        ANTHROPIC_BASE_URL_ENV,
        ANTHROPIC_MODEL_ENV
    )
);
