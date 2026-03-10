#[derive(Clone, Copy)]
pub(super) struct ClientEnv {
    pub(super) api_key: &'static str,
    pub(super) base_url: &'static str,
    pub(super) model: &'static str,
}

impl ClientEnv {
    pub(super) const fn new(
        api_key: &'static str,
        base_url: &'static str,
        model: &'static str,
    ) -> Self {
        Self {
            api_key,
            base_url,
            model,
        }
    }
}

pub(super) fn build_base_from_env(
    mut builder: crate::base_client_builder::BaseClientBuilder,
    env: ClientEnv,
) -> Result<crate::base_client_builder::BaseClientBuilder, crate::runtime_error::RuntimeError> {
    let _ = dotenvy::dotenv();

    builder.api_key = Some(require_env(env.api_key)?);
    if let Some(base_url) = read_env(env.base_url) {
        builder.base_url = Some(base_url);
    }
    if let Some(default_model) = read_env(env.model) {
        builder.default_model = Some(default_model);
    }

    Ok(builder)
}

fn read_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn require_env(key: &str) -> Result<String, crate::runtime_error::RuntimeError> {
    read_env(key).ok_or_else(|| {
        crate::runtime_error::RuntimeError::configuration(format!("missing required env var {key}"))
    })
}

macro_rules! impl_provider_client {
    (
        client = $client:ident,
        builder = $builder:ident,
        provider = $provider:expr,
        constructor = $constructor:ident,
        env = $env:expr
    ) => {
        impl $client {
            /// Creates a builder for this provider client.
            pub fn builder() -> $builder {
                $builder::default()
            }

            /// Builds a client from provider-specific environment variables.
            ///
            /// A `.env` file is loaded if present.
            pub fn from_env() -> Result<Self, crate::runtime_error::RuntimeError> {
                $builder {
                    inner: super::common::build_base_from_env(
                        crate::base_client_builder::BaseClientBuilder::default(),
                        $env,
                    )?,
                }
                .build()
            }

            /// Returns the non-streaming API for this provider.
            pub fn messages(&self) -> crate::direct_messages_api::DirectMessagesApi<'_> {
                self.inner.messages()
            }

            /// Returns the streaming API for this provider.
            pub fn streaming(&self) -> crate::direct_streaming_api::DirectStreamingApi<'_> {
                self.inner.streaming()
            }

            /// Sends a fully-formed request directly to this provider.
            pub async fn send(
                &self,
                request: agent_core::Request,
            ) -> Result<agent_core::Response, crate::runtime_error::RuntimeError> {
                self.inner.send(request).await
            }

            /// Sends a fully-formed request and returns attempt metadata.
            pub async fn send_with_meta(
                &self,
                request: agent_core::Request,
            ) -> Result<
                (agent_core::Response, crate::types::ResponseMeta),
                crate::runtime_error::RuntimeError,
            > {
                self.inner.send_with_meta(request).await
            }
        }

        impl $builder {
            /// Sets the provider API key.
            pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
                self.inner.api_key = Some(api_key.into());
                self
            }

            /// Sets the provider base URL override.
            pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
                self.inner.base_url = Some(base_url.into());
                self
            }

            /// Sets the default model used when requests omit one.
            pub fn default_model(mut self, default_model: impl Into<String>) -> Self {
                self.inner.default_model = Some(default_model.into());
                self
            }

            /// Sets the transport retry policy.
            pub fn retry_policy(mut self, retry_policy: agent_transport::RetryPolicy) -> Self {
                self.inner.retry_policy = Some(retry_policy);
                self
            }

            /// Sets the non-streaming request timeout.
            pub fn request_timeout(mut self, timeout: std::time::Duration) -> Self {
                self.inner.request_timeout = Some(timeout);
                self
            }

            /// Sets the stream timeout configuration.
            pub fn stream_timeout(mut self, timeout: std::time::Duration) -> Self {
                self.inner.stream_timeout = Some(timeout);
                self
            }

            /// Supplies a preconfigured `reqwest` client.
            pub fn client(mut self, client: reqwest::Client) -> Self {
                self.inner.client = Some(client);
                self
            }

            /// Sets a client-level observer.
            pub fn observer(
                mut self,
                observer: std::sync::Arc<dyn crate::observer::RuntimeObserver>,
            ) -> Self {
                self.inner.observer = Some(observer);
                self
            }

            /// Builds the provider client.
            pub fn build(self) -> Result<$client, crate::runtime_error::RuntimeError> {
                let provider_runtime = self.inner.build_runtime($provider)?;
                Ok($client {
                    inner: crate::provider_client::ProviderClient::new(provider_runtime),
                })
            }
        }

        /// Convenience constructor returning the provider-specific builder.
        pub fn $constructor() -> $builder {
            $client::builder()
        }
    };
}

pub(super) use impl_provider_client;
