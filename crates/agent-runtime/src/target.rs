use agent_core::ProviderId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub provider: ProviderId,
    pub model: Option<String>,
}

impl Target {
    pub fn new(provider: ProviderId) -> Self {
        Self {
            provider,
            model: None,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}
