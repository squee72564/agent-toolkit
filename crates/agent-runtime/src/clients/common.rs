use crate::clients::base_client_builder::BaseClientBuilder;
use crate::runtime_error::RuntimeError;

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
    mut builder: BaseClientBuilder,
    env: ClientEnv,
) -> Result<BaseClientBuilder, RuntimeError> {
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

fn require_env(key: &str) -> Result<String, RuntimeError> {
    read_env(key)
        .ok_or_else(|| RuntimeError::configuration(format!("missing required env var {key}")))
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
            pub fn from_env() -> Result<Self, $crate::runtime_error::RuntimeError> {
                $builder {
                    inner: super::common::build_base_from_env(BaseClientBuilder::default(), $env)?,
                }
                .build()
            }

            /// Returns the non-streaming API for this provider.
            pub fn messages(&self) -> $crate::api::DirectMessagesApi<'_> {
                self.inner.messages()
            }

            /// Returns the streaming API for this provider.
            pub fn streaming(&self) -> $crate::api::DirectStreamingApi<'_> {
                self.inner.streaming()
            }

            /// Executes an explicit semantic task against this provider.
            pub async fn execute(
                &self,
                task: agent_core::TaskRequest,
                execution: $crate::execution_options::ExecutionOptions,
            ) -> Result<agent_core::Response, $crate::runtime_error::RuntimeError> {
                self.inner.execute(task, execution).await
            }

            /// Executes an explicit semantic task and returns attempt metadata.
            pub async fn execute_with_meta(
                &self,
                task: agent_core::TaskRequest,
                execution: $crate::execution_options::ExecutionOptions,
            ) -> Result<
                (agent_core::Response, $crate::types::ResponseMeta),
                $crate::runtime_error::RuntimeError,
            > {
                self.inner.execute_with_meta(task, execution).await
            }

            /// Executes an explicit semantic task against an explicit
            /// single-attempt route for this client.
            pub async fn execute_on_attempt(
                &self,
                task: agent_core::TaskRequest,
                attempt: $crate::AttemptSpec,
                execution: $crate::execution_options::ExecutionOptions,
            ) -> Result<agent_core::Response, $crate::runtime_error::RuntimeError> {
                self.inner
                    .execute_on_attempt(task, attempt, execution)
                    .await
            }

            /// Executes an explicit semantic task against an explicit
            /// single-attempt route for this client and returns attempt
            /// metadata.
            pub async fn execute_on_attempt_with_meta(
                &self,
                task: agent_core::TaskRequest,
                attempt: $crate::AttemptSpec,
                execution: $crate::execution_options::ExecutionOptions,
            ) -> Result<
                (agent_core::Response, $crate::types::ResponseMeta),
                $crate::runtime_error::RuntimeError,
            > {
                self.inner
                    .execute_on_attempt_with_meta(task, attempt, execution)
                    .await
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
                observer: std::sync::Arc<dyn $crate::observability::RuntimeObserver>,
            ) -> Self {
                self.inner.observer = Some(observer);
                self
            }

            /// Builds the provider client.
            pub fn build(self) -> Result<$client, $crate::runtime_error::RuntimeError> {
                let provider_runtime = self.inner.build_runtime(
                    $provider,
                    match $provider {
                        agent_core::ProviderKind::OpenAi => {
                            agent_core::ProviderInstanceId::openai_default()
                        }
                        agent_core::ProviderKind::Anthropic => {
                            agent_core::ProviderInstanceId::anthropic_default()
                        }
                        agent_core::ProviderKind::OpenRouter => {
                            agent_core::ProviderInstanceId::openrouter_default()
                        }
                        agent_core::ProviderKind::GenericOpenAiCompatible => {
                            agent_core::ProviderInstanceId::generic_openai_compatible_default()
                        }
                    },
                )?;
                Ok($client {
                    inner: $crate::provider::ProviderClient::new(provider_runtime),
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
