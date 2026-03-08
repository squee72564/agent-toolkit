use std::time::Duration;

use agent_transport::RetryPolicy;

#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub api_key: String,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub retry_policy: Option<RetryPolicy>,
    pub request_timeout: Option<Duration>,
    pub stream_timeout: Option<Duration>,
}

impl ProviderConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            ..Self::default()
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn with_default_model(mut self, default_model: impl Into<String>) -> Self {
        self.default_model = Some(default_model.into());
        self
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = Some(retry_policy);
        self
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    pub fn with_stream_timeout(mut self, timeout: Duration) -> Self {
        self.stream_timeout = Some(timeout);
        self
    }
}
