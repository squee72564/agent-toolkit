use std::{sync::Arc, time::Duration};

use agent_core::{ProviderInstanceId, ProviderKind};
use agent_providers::adapter::adapter_for;
use agent_transport::{HttpTransport, RetryPolicy};
use reqwest::header::HeaderName;

use crate::observability::RuntimeObserver;
use crate::provider::{ProviderConfig, RegisteredProvider};
use crate::provider_runtime::ProviderRuntime;
use crate::runtime_error::RuntimeError;

#[derive(Clone, Default)]
pub(crate) struct BaseClientBuilder {
    pub(crate) api_key: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) default_model: Option<String>,
    pub(crate) request_id_header: Option<HeaderName>,
    pub(crate) retry_policy: Option<RetryPolicy>,
    pub(crate) request_timeout: Option<Duration>,
    pub(crate) stream_timeout: Option<Duration>,
    pub(crate) client: Option<reqwest::Client>,
    pub(crate) observer: Option<Arc<dyn RuntimeObserver>>,
}

impl BaseClientBuilder {
    pub(crate) fn from_provider_config(config: ProviderConfig) -> Self {
        Self {
            api_key: Some(config.api_key),
            base_url: config.base_url,
            default_model: config.default_model,
            request_id_header: config.request_id_header,
            retry_policy: config.retry_policy,
            request_timeout: config.request_timeout,
            stream_timeout: config.stream_timeout,
            client: None,
            observer: None,
        }
    }

    pub(crate) fn build_runtime(
        self,
        kind: ProviderKind,
        instance_id: ProviderInstanceId,
    ) -> Result<ProviderRuntime, RuntimeError> {
        let adapter = adapter_for(kind);
        let api_key = self.api_key.ok_or_else(|| {
            RuntimeError::configuration(format!("missing API key for provider {kind:?}"))
        })?;
        if api_key.trim().is_empty() {
            return Err(RuntimeError::configuration(format!(
                "API key is empty for provider {kind:?}"
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
        if let Some(retry_policy) = self.retry_policy.clone() {
            transport_builder = transport_builder.retry_policy(retry_policy);
        }
        if let Some(timeout) = self.request_timeout {
            transport_builder = transport_builder.request_timeout(timeout);
        }
        if let Some(timeout) = self.stream_timeout {
            transport_builder = transport_builder.stream_timeout(timeout);
        }

        let transport = transport_builder.build();
        let registered = RegisteredProvider::new(
            instance_id.clone(),
            kind,
            ProviderConfig {
                api_key,
                base_url: self.base_url,
                default_model: self.default_model,
                request_id_header: self.request_id_header,
                retry_policy: self.retry_policy,
                request_timeout: self.request_timeout,
                stream_timeout: self.stream_timeout,
            },
        );
        let platform = registered.platform_config(adapter.descriptor())?;

        Ok(ProviderRuntime {
            instance_id,
            kind,
            registered,
            adapter,
            platform,
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
            .field("request_id_header", &self.request_id_header)
            .field("retry_policy", &self.retry_policy)
            .field("request_timeout", &self.request_timeout)
            .field("stream_timeout", &self.stream_timeout)
            .field("client", &self.client.as_ref().map(|_| "configured"))
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
}
