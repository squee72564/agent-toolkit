use crate::base_client_builder::BaseClientBuilder;
use crate::clients::common::{ClientEnv, impl_provider_client};
use crate::provider_client::ProviderClient;
use agent_core::ProviderId;

const OPENROUTER_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
const OPENROUTER_BASE_URL_ENV: &str = "OPENROUTER_BASE_URL";
const OPENROUTER_MODEL_ENV: &str = "OPENROUTER_MODEL";

/// Direct client for OpenRouter requests.
#[derive(Debug, Clone)]
pub struct OpenRouterClient {
    inner: ProviderClient,
}

/// Builder for [`OpenRouterClient`].
#[derive(Debug, Clone, Default)]
pub struct OpenRouterClientBuilder {
    inner: BaseClientBuilder,
}

impl_provider_client!(
    client = OpenRouterClient,
    builder = OpenRouterClientBuilder,
    provider = ProviderId::OpenRouter,
    constructor = openrouter,
    env = ClientEnv::new(
        OPENROUTER_API_KEY_ENV,
        OPENROUTER_BASE_URL_ENV,
        OPENROUTER_MODEL_ENV
    )
);
