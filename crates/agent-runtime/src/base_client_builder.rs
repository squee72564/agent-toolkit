use std::{sync::Arc, time::Duration};

use agent_core::ProviderId;
use agent_providers::adapter::adapter_for;
use agent_transport::{HttpTransport, RetryPolicy};

use crate::observer::RuntimeObserver;
use crate::provider_config::ProviderConfig;
use crate::provider_runtime::ProviderRuntime;
use crate::runtime_error::RuntimeError;

#[derive(Clone, Default)]
pub struct BaseClientBuilder {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub retry_policy: Option<RetryPolicy>,
    pub timeout: Option<Duration>,
    pub client: Option<reqwest::Client>,
    pub observer: Option<Arc<dyn RuntimeObserver>>,
}

impl BaseClientBuilder {
    pub fn from_provider_config(config: ProviderConfig) -> Self {
        Self {
            api_key: Some(config.api_key),
            base_url: config.base_url,
            default_model: config.default_model,
            retry_policy: config.retry_policy,
            timeout: config.timeout,
            client: None,
            observer: None,
        }
    }

    pub fn build_runtime(self, provider: ProviderId) -> Result<ProviderRuntime, RuntimeError> {
        let adapter = adapter_for(provider);
        let api_key = self.api_key.ok_or_else(|| {
            RuntimeError::configuration(format!("missing API key for provider {provider:?}"))
        })?;
        if api_key.trim().is_empty() {
            return Err(RuntimeError::configuration(format!(
                "API key is empty for provider {provider:?}"
            )));
        }

        let reqwest_client = if let Some(client) = self.client {
            client
        } else {
            reqwest::Client::builder()
                .build()
                .map_err(|error| RuntimeError::configuration(error.to_string()))?
        };

        let mut transport_builder = HttpTransport::builder(reqwest_client);
        if let Some(retry_policy) = self.retry_policy {
            transport_builder = transport_builder.retry_policy(retry_policy);
        }
        if let Some(timeout) = self.timeout {
            transport_builder = transport_builder.timeout(timeout);
        }

        let transport = transport_builder.build();
        let base_url = self
            .base_url
            .unwrap_or_else(|| adapter.default_base_url().to_string());
        let platform = adapter
            .platform_config(base_url)
            .map_err(|error| RuntimeError::configuration(error.message))?;

        Ok(ProviderRuntime {
            provider,
            adapter,
            platform,
            auth_token: api_key,
            default_model: self.default_model,
            transport,
            observer: self.observer,
        })
    }
}

impl std::fmt::Debug for BaseClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BaseClientBuilder")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("retry_policy", &self.retry_policy)
            .field("timeout", &self.timeout)
            .field("client", &self.client.as_ref().map(|_| "configured"))
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
}
