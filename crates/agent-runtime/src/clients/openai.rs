use std::{sync::Arc, time::Duration};

use agent_core::{ProviderId, Request, Response};
use agent_transport::RetryPolicy;

use crate::base_client_builder::BaseClientBuilder;
use crate::direct_messages_api::DirectMessagesApi;
use crate::direct_streaming_api::DirectStreamingApi;
use crate::observer::RuntimeObserver;
use crate::provider_client::ProviderClient;
use crate::runtime_error::RuntimeError;
use crate::types::ResponseMeta;

const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const OPENAI_BASE_URL_ENV: &str = "OPENAI_BASE_URL";
const OPENAI_MODEL_ENV: &str = "OPENAI_MODEL";

#[derive(Debug, Clone)]
pub struct OpenAiClient {
    inner: ProviderClient,
}

impl OpenAiClient {
    pub fn builder() -> OpenAiClientBuilder {
        OpenAiClientBuilder::default()
    }

    pub fn from_env() -> Result<Self, RuntimeError> {
        let _ = dotenvy::dotenv();

        let mut builder = Self::builder().api_key(require_env(OPENAI_API_KEY_ENV)?);
        if let Some(base_url) = read_env(OPENAI_BASE_URL_ENV) {
            builder = builder.base_url(base_url);
        }
        if let Some(default_model) = read_env(OPENAI_MODEL_ENV) {
            builder = builder.default_model(default_model);
        }

        builder.build()
    }

    pub fn messages(&self) -> DirectMessagesApi<'_> {
        self.inner.messages()
    }

    pub fn streaming(&self) -> DirectStreamingApi<'_> {
        self.inner.streaming()
    }

    pub async fn send(&self, request: Request) -> Result<Response, RuntimeError> {
        self.inner.send(request).await
    }

    pub async fn send_with_meta(
        &self,
        request: Request,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        self.inner.send_with_meta(request).await
    }
}

#[derive(Debug, Clone, Default)]
pub struct OpenAiClientBuilder {
    inner: BaseClientBuilder,
}

impl OpenAiClientBuilder {
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.inner.api_key = Some(api_key.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.inner.base_url = Some(base_url.into());
        self
    }

    pub fn default_model(mut self, default_model: impl Into<String>) -> Self {
        self.inner.default_model = Some(default_model.into());
        self
    }

    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.inner.retry_policy = Some(retry_policy);
        self
    }

    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.inner.request_timeout = Some(timeout);
        self
    }

    pub fn stream_timeout(mut self, timeout: Duration) -> Self {
        self.inner.stream_timeout = Some(timeout);
        self
    }

    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.inner.client = Some(client);
        self
    }

    pub fn observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.inner.observer = Some(observer);
        self
    }

    pub fn build(self) -> Result<OpenAiClient, RuntimeError> {
        let provider_runtime = self.inner.build_runtime(ProviderId::OpenAi)?;
        Ok(OpenAiClient {
            inner: ProviderClient::new(provider_runtime),
        })
    }
}

pub fn openai() -> OpenAiClientBuilder {
    OpenAiClient::builder()
}

fn read_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
}

fn require_env(key: &str) -> Result<String, RuntimeError> {
    read_env(key)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RuntimeError::configuration(format!("missing required env var {key}")))
}
