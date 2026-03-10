use agent_core::ProviderId;

/// Concrete provider/model destination for routed execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Provider selected for this target.
    pub provider: ProviderId,
    /// Optional model override for this provider.
    pub model: Option<String>,
}

impl Target {
    /// Creates a target for a provider, leaving model resolution to request or
    /// provider defaults.
    pub fn new(provider: ProviderId) -> Self {
        Self {
            provider,
            model: None,
        }
    }

    /// Sets the model override for the target.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}
