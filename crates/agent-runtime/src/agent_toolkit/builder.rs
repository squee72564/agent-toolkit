use std::{collections::HashMap, sync::Arc};

use agent_core::{ProviderInstanceId, ProviderKind};

use crate::agent_toolkit::AgentToolkit;
use crate::clients::BaseClientBuilder;
use crate::observability::RuntimeObserver;
use crate::provider::{ProviderClient, ProviderConfig};
use crate::runtime_error::RuntimeError;

#[derive(Clone)]
struct ProviderRegistration {
    kind: ProviderKind,
    instance_id: ProviderInstanceId,
    config: ProviderConfig,
}

/// Builder for an [`AgentToolkit`].
///
/// At least one provider must be configured before calling [`Self::build`].
#[derive(Clone, Default)]
pub struct AgentToolkitBuilder {
    registrations: Vec<ProviderRegistration>,
    observer: Option<Arc<dyn RuntimeObserver>>,
}

impl AgentToolkitBuilder {
    /// Registers an OpenAI provider configuration.
    #[cfg(feature = "openai")]
    pub fn with_openai(mut self, config: ProviderConfig) -> Self {
        self = self.with_openai_instance(ProviderInstanceId::openai_default(), config);
        self
    }

    /// Registers an Anthropic provider configuration.
    #[cfg(feature = "anthropic")]
    pub fn with_anthropic(mut self, config: ProviderConfig) -> Self {
        self = self.with_anthropic_instance(ProviderInstanceId::anthropic_default(), config);
        self
    }

    /// Registers an OpenRouter provider configuration.
    #[cfg(feature = "openrouter")]
    pub fn with_openrouter(mut self, config: ProviderConfig) -> Self {
        self = self.with_openrouter_instance(ProviderInstanceId::openrouter_default(), config);
        self
    }

    /// Registers an OpenAI provider configuration for a specific instance id.
    #[cfg(feature = "openai")]
    pub fn with_openai_instance(
        mut self,
        instance_id: impl Into<ProviderInstanceId>,
        config: ProviderConfig,
    ) -> Self {
        self.upsert_registration(ProviderKind::OpenAi, instance_id.into(), config);
        self
    }

    /// Registers an Anthropic provider configuration for a specific instance id.
    #[cfg(feature = "anthropic")]
    pub fn with_anthropic_instance(
        mut self,
        instance_id: impl Into<ProviderInstanceId>,
        config: ProviderConfig,
    ) -> Self {
        self.upsert_registration(ProviderKind::Anthropic, instance_id.into(), config);
        self
    }

    /// Registers an OpenRouter provider configuration for a specific instance id.
    #[cfg(feature = "openrouter")]
    pub fn with_openrouter_instance(
        mut self,
        instance_id: impl Into<ProviderInstanceId>,
        config: ProviderConfig,
    ) -> Self {
        self.upsert_registration(ProviderKind::OpenRouter, instance_id.into(), config);
        self
    }

    /// Registers a generic OpenAI-compatible provider configuration for a specific instance id.
    pub fn with_generic_openai_compatible_instance(
        mut self,
        instance_id: impl Into<ProviderInstanceId>,
        config: ProviderConfig,
    ) -> Self {
        self.upsert_registration(
            ProviderKind::GenericOpenAiCompatible,
            instance_id.into(),
            config,
        );
        self
    }

    /// Registers a provider configuration for an explicit provider instance id.
    pub fn with_provider_instance(
        mut self,
        kind: ProviderKind,
        instance_id: impl Into<ProviderInstanceId>,
        config: ProviderConfig,
    ) -> Self {
        self.upsert_registration(kind, instance_id.into(), config);
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
            registrations,
            observer,
        } = self;
        let mut clients = HashMap::new();

        for registration in registrations {
            let client = build_provider_client(
                registration.kind,
                registration.instance_id.clone(),
                registration.config,
                observer.clone(),
            )?;
            clients.insert(registration.instance_id, client);
        }

        if clients.is_empty() {
            return Err(RuntimeError::configuration(
                "at least one provider must be configured",
            ));
        }

        Ok(AgentToolkit { clients, observer })
    }

    fn upsert_registration(
        &mut self,
        kind: ProviderKind,
        instance_id: ProviderInstanceId,
        config: ProviderConfig,
    ) {
        if let Some(existing) = self
            .registrations
            .iter_mut()
            .find(|existing| existing.instance_id == instance_id)
        {
            existing.kind = kind;
            existing.config = config;
            return;
        }

        self.registrations.push(ProviderRegistration {
            kind,
            instance_id,
            config,
        });
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
