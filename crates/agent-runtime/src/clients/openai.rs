use crate::base_client_builder::BaseClientBuilder;
use crate::clients::common::{ClientEnv, impl_provider_client};
use crate::provider_client::ProviderClient;
use agent_core::ProviderId;

const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const OPENAI_BASE_URL_ENV: &str = "OPENAI_BASE_URL";
const OPENAI_MODEL_ENV: &str = "OPENAI_MODEL";

/// Direct client for the OpenAI-compatible runtime path.
#[derive(Debug, Clone)]
pub struct OpenAiClient {
    inner: ProviderClient,
}

/// Builder for [`OpenAiClient`].
#[derive(Debug, Clone, Default)]
pub struct OpenAiClientBuilder {
    inner: BaseClientBuilder,
}

impl_provider_client!(
    client = OpenAiClient,
    builder = OpenAiClientBuilder,
    provider = ProviderId::OpenAi,
    constructor = openai,
    env = ClientEnv::new(OPENAI_API_KEY_ENV, OPENAI_BASE_URL_ENV, OPENAI_MODEL_ENV)
);
