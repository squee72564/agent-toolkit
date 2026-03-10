use std::{collections::HashMap, sync::Arc};

use agent_core::ProviderId;

use crate::agent_toolkit::AgentToolkit;
use crate::base_client_builder::BaseClientBuilder;
use crate::observer::RuntimeObserver;
use crate::provider_client::ProviderClient;
use crate::provider_config::ProviderConfig;
use crate::runtime_error::RuntimeError;

#[derive(Clone, Default)]
pub struct AgentToolkitBuilder {
    openai: Option<ProviderConfig>,
    anthropic: Option<ProviderConfig>,
    openrouter: Option<ProviderConfig>,
    observer: Option<Arc<dyn RuntimeObserver>>,
}

impl AgentToolkitBuilder {
    pub fn with_openai(mut self, config: ProviderConfig) -> Self {
        self.openai = Some(config);
        self
    }

    pub fn with_anthropic(mut self, config: ProviderConfig) -> Self {
        self.anthropic = Some(config);
        self
    }

    pub fn with_openrouter(mut self, config: ProviderConfig) -> Self {
        self.openrouter = Some(config);
        self
    }

    pub fn observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn build(self) -> Result<AgentToolkit, RuntimeError> {
        let Self {
            openai,
            anthropic,
            openrouter,
            observer,
        } = self;
        let mut clients = HashMap::new();

        if let Some(config) = openai {
            clients.insert(
                ProviderId::OpenAi,
                build_provider_client(ProviderId::OpenAi, config, observer.clone())?,
            );
        }
        if let Some(config) = anthropic {
            clients.insert(
                ProviderId::Anthropic,
                build_provider_client(ProviderId::Anthropic, config, observer.clone())?,
            );
        }
        if let Some(config) = openrouter {
            clients.insert(
                ProviderId::OpenRouter,
                build_provider_client(ProviderId::OpenRouter, config, observer.clone())?,
            );
        }

        if clients.is_empty() {
            return Err(RuntimeError::configuration(
                "at least one provider must be configured",
            ));
        }

        Ok(AgentToolkit { clients, observer })
    }
}

fn build_provider_client(
    provider: ProviderId,
    config: ProviderConfig,
    observer: Option<Arc<dyn RuntimeObserver>>,
) -> Result<ProviderClient, RuntimeError> {
    let mut runtime_builder = BaseClientBuilder::from_provider_config(config);
    runtime_builder.observer = observer;
    let runtime = runtime_builder.build_runtime(provider)?;
    Ok(ProviderClient::new(runtime))
}
