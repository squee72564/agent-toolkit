use agent_core::ProviderInstanceId;

/// Concrete provider/model destination for routed execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Registered provider instance selected for this target.
    pub instance: ProviderInstanceId,
    /// Optional model override for this provider.
    pub model: Option<String>,
}

impl Target {
    /// Creates a target for a provider instance, leaving model resolution to
    /// request or provider defaults.
    pub fn new(instance: impl Into<ProviderInstanceId>) -> Self {
        Self {
            instance: instance.into(),
            model: None,
        }
    }

    /// Sets the model override for the target.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}
