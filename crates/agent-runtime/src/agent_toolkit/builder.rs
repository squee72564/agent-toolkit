use std::{collections::HashMap, sync::Arc};

use agent_core::{ProviderInstanceId, ProviderKind};

use crate::agent_toolkit::AgentToolkit;
use crate::base_client_builder::BaseClientBuilder;
use crate::observer::RuntimeObserver;
use crate::provider_client::ProviderClient;
use crate::provider_config::ProviderConfig;
use crate::runtime_error::RuntimeError;

/// Builder for an [`AgentToolkit`].
///
/// At least one provider must be configured before calling [`Self::build`].
#[derive(Clone, Default)]
pub struct AgentToolkitBuilder {
    openai: Option<ProviderConfig>,
    anthropic: Option<ProviderConfig>,
    openrouter: Option<ProviderConfig>,
    observer: Option<Arc<dyn RuntimeObserver>>,
}

impl AgentToolkitBuilder {
    /// Registers an OpenAI provider configuration.
    pub fn with_openai(mut self, config: ProviderConfig) -> Self {
        self.openai = Some(config);
        self
    }

    /// Registers an Anthropic provider configuration.
    pub fn with_anthropic(mut self, config: ProviderConfig) -> Self {
        self.anthropic = Some(config);
        self
    }

    /// Registers an OpenRouter provider configuration.
    pub fn with_openrouter(mut self, config: ProviderConfig) -> Self {
        self.openrouter = Some(config);
        self
    }

    /// Sets a toolkit-level observer used for routed requests unless a
    /// request-scoped observer override is provided.
    pub fn observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// Builds the toolkit.
    ///
    /// Returns a configuration error when no providers were registered or when
    /// any provider configuration is invalid.
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
                ProviderInstanceId::new("openai-default"),
                build_provider_client(
                    ProviderKind::OpenAi,
                    ProviderInstanceId::new("openai-default"),
                    config,
                    observer.clone(),
                )?,
            );
        }
        if let Some(config) = anthropic {
            clients.insert(
                ProviderInstanceId::new("anthropic-default"),
                build_provider_client(
                    ProviderKind::Anthropic,
                    ProviderInstanceId::new("anthropic-default"),
                    config,
                    observer.clone(),
                )?,
            );
        }
        if let Some(config) = openrouter {
            clients.insert(
                ProviderInstanceId::new("openrouter-default"),
                build_provider_client(
                    ProviderKind::OpenRouter,
                    ProviderInstanceId::new("openrouter-default"),
                    config,
                    observer.clone(),
                )?,
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
    provider: ProviderKind,
    instance_id: ProviderInstanceId,
    config: ProviderConfig,
    observer: Option<Arc<dyn RuntimeObserver>>,
) -> Result<ProviderClient, RuntimeError> {
    let mut runtime_builder = BaseClientBuilder::from_provider_config(config);
    runtime_builder.observer = observer;
    let runtime = runtime_builder.build_runtime(provider, instance_id)?;
    Ok(ProviderClient::new(runtime))
}
