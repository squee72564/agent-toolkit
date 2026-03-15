use std::time::Duration;

use agent_transport::RetryPolicy;
use reqwest::header::HeaderName;

/// Provider configuration shared by direct clients and [`crate::AgentToolkit`].
#[derive(Clone, Default)]
pub struct ProviderConfig {
    /// API key or bearer token used for provider authentication.
    pub api_key: String,
    /// Optional override for the provider base URL.
    pub base_url: Option<String>,
    /// Default model used when requests do not specify one explicitly.
    pub default_model: Option<String>,
    /// Optional override for response request-id header extraction.
    pub request_id_header: Option<HeaderName>,
    /// Optional transport retry policy.
    pub retry_policy: Option<RetryPolicy>,
    /// Optional timeout for non-streaming requests.
    pub request_timeout: Option<Duration>,
    /// Optional timeout for stream setup and streaming activity.
    pub stream_timeout: Option<Duration>,
}

impl ProviderConfig {
    /// Creates a provider configuration with the required API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            ..Self::default()
        }
    }

    /// Sets the provider base URL override.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Sets the default model used when requests omit a model.
    pub fn with_default_model(mut self, default_model: impl Into<String>) -> Self {
        self.default_model = Some(default_model.into());
        self
    }

    /// Sets the default response request-id header for this provider instance.
    pub fn with_request_id_header(mut self, request_id_header: HeaderName) -> Self {
        self.request_id_header = Some(request_id_header);
        self
    }

    /// Sets the transport retry policy.
    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = Some(retry_policy);
        self
    }

    /// Sets the non-streaming request timeout.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Sets the stream timeout configuration.
    pub fn with_stream_timeout(mut self, timeout: Duration) -> Self {
        self.stream_timeout = Some(timeout);
        self
    }
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("request_id_header", &self.request_id_header)
            .field("retry_policy", &self.retry_policy)
            .field("request_timeout", &self.request_timeout)
            .field("stream_timeout", &self.stream_timeout)
            .finish()
    }
}
