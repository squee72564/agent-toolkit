use std::{collections::HashMap, sync::Arc};

use agent_core::{ProviderId, Request, Response};

use crate::base_client_builder::BaseClientBuilder;
use crate::observer::{RuntimeObserver, resolve_observer_for_request, safe_call_observer};
use crate::provider_client::ProviderClient;
use crate::provider_config::ProviderConfig;
use crate::provider_runtime::ProviderAttemptOutcome;
use crate::router_messages_api::RouterMessagesApi;
use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::send_options::SendOptions;
use crate::target::Target;
use crate::types::{
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, RequestEndEvent,
    RequestStartEvent, ResponseMeta,
};

#[derive(Clone)]
pub struct AgentToolkit {
    pub clients: HashMap<ProviderId, ProviderClient>,
    pub observer: Option<Arc<dyn RuntimeObserver>>,
}

impl std::fmt::Debug for AgentToolkit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentToolkit")
            .field("clients", &self.clients)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
}

impl AgentToolkit {
    pub fn builder() -> AgentToolkitBuilder {
        AgentToolkitBuilder::default()
    }

    pub fn messages(&self) -> RouterMessagesApi<'_> {
        RouterMessagesApi { toolkit: self }
    }

    pub async fn send(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<Response, RuntimeError> {
        self.send_with_meta(request, options)
            .await
            .map(|(response, _)| response)
    }

    pub async fn send_with_meta(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<(Response, ResponseMeta), RuntimeError> {
        let request_started_at = std::time::Instant::now();
        let targets = self.resolve_targets(&options)?;
        let first_client_observer = targets
            .first()
            .and_then(|target| self.clients.get(&target.provider))
            .and_then(|client| client.runtime.observer.as_ref());
        let request_observer = resolve_observer_for_request(
            first_client_observer,
            self.observer.as_ref(),
            options.observer.as_ref(),
        );
        let request_start_event = RequestStartEvent {
            request_id: None,
            provider: targets.first().map(|target| target.provider),
            model: targets
                .first()
                .and_then(|target| event_model(target.model.as_deref(), &request.model_id)),
            target_index: None,
            attempt_index: None,
            elapsed: request_started_at.elapsed(),
            first_target: targets.first().map(|target| target.provider),
            resolved_target_count: targets.len(),
        };
        safe_call_observer(request_observer, |observer| {
            observer.on_request_start(&request_start_event)
        });

        let fallback_policy = options.fallback_policy.clone();
        let mut attempts = Vec::new();
        let mut last_error: Option<RuntimeError> = None;

        let request_model_id = request.model_id.clone();
        let mut request = Some(request);

        for (index, target) in targets.iter().enumerate() {
            let Some(client) = self.clients.get(&target.provider) else {
                let error = RuntimeError::target_resolution(format!(
                    "provider {:?} is not registered",
                    target.provider
                ));
                let request_end_event = RequestEndEvent {
                    request_id: error.request_id.clone(),
                    provider: Some(target.provider),
                    model: event_model(target.model.as_deref(), &request_model_id),
                    target_index: Some(index),
                    attempt_index: Some(index),
                    elapsed: request_started_at.elapsed(),
                    status_code: error.status_code,
                    error_kind: Some(error.kind),
                    error_message: Some(error.message.clone()),
                };
                safe_call_observer(request_observer, |runtime_observer| {
                    runtime_observer.on_request_end(&request_end_event);
                });
                return Err(error);
            };
            let observer = resolve_observer_for_request(
                client.runtime.observer.as_ref(),
                self.observer.as_ref(),
                options.observer.as_ref(),
            );
            let attempt_started_at = std::time::Instant::now();
            let attempt_start_event = AttemptStartEvent {
                request_id: None,
                provider: Some(target.provider),
                model: event_model(target.model.as_deref(), &request_model_id),
                target_index: Some(index),
                attempt_index: Some(index),
                elapsed: attempt_started_at.elapsed(),
            };
            safe_call_observer(observer, |runtime_observer| {
                runtime_observer.on_attempt_start(&attempt_start_event);
            });

            let is_last = index + 1 >= targets.len();
            let Some(attempt_request) = (if is_last {
                request.take()
            } else {
                request.as_ref().cloned()
            }) else {
                let error = RuntimeError::target_resolution(
                    "request state was exhausted before completing fallback attempts",
                );
                let request_end_event = RequestEndEvent {
                    request_id: error.request_id.clone(),
                    provider: Some(target.provider),
                    model: event_model(target.model.as_deref(), &request_model_id),
                    target_index: Some(index),
                    attempt_index: Some(index),
                    elapsed: request_started_at.elapsed(),
                    status_code: error.status_code,
                    error_kind: Some(error.kind),
                    error_message: Some(error.message.clone()),
                };
                safe_call_observer(request_observer, |runtime_observer| {
                    runtime_observer.on_request_end(&request_end_event);
                });
                return Err(error);
            };

            let attempt = client
                .runtime
                .execute_attempt(
                    attempt_request,
                    target.model.as_deref(),
                    options.metadata.clone(),
                )
                .await;

            match attempt {
                ProviderAttemptOutcome::Success { response, meta } => {
                    let attempt_success_event = AttemptSuccessEvent {
                        request_id: meta.request_id.clone(),
                        provider: Some(meta.provider),
                        model: Some(meta.model.clone()),
                        target_index: Some(index),
                        attempt_index: Some(index),
                        elapsed: attempt_started_at.elapsed(),
                        status_code: meta.status_code,
                    };
                    safe_call_observer(observer, |runtime_observer| {
                        runtime_observer.on_attempt_success(&attempt_success_event);
                    });

                    attempts.push(meta.clone());
                    let response_meta = ResponseMeta {
                        selected_provider: meta.provider,
                        selected_model: meta.model,
                        status_code: meta.status_code,
                        request_id: meta.request_id.clone(),
                        attempts,
                    };

                    let request_end_event = RequestEndEvent {
                        request_id: response_meta.request_id.clone(),
                        provider: Some(response_meta.selected_provider),
                        model: Some(response_meta.selected_model.clone()),
                        target_index: Some(index),
                        attempt_index: Some(index),
                        elapsed: request_started_at.elapsed(),
                        status_code: response_meta.status_code,
                        error_kind: None,
                        error_message: None,
                    };
                    safe_call_observer(observer, |runtime_observer| {
                        runtime_observer.on_request_end(&request_end_event);
                    });
                    return Ok((response, response_meta));
                }
                ProviderAttemptOutcome::Failure { error, meta } => {
                    let attempt_failure_event = AttemptFailureEvent {
                        request_id: meta.request_id.clone(),
                        provider: Some(meta.provider),
                        model: Some(meta.model.clone()),
                        target_index: Some(index),
                        attempt_index: Some(index),
                        elapsed: attempt_started_at.elapsed(),
                        error_kind: meta.error_kind,
                        error_message: meta.error_message.clone(),
                    };
                    safe_call_observer(observer, |runtime_observer| {
                        runtime_observer.on_attempt_failure(&attempt_failure_event);
                    });

                    attempts.push(meta);
                    let should_continue = index + 1 < targets.len()
                        && fallback_policy
                            .as_ref()
                            .is_some_and(|policy| policy.should_fallback(&error));
                    last_error = Some(error);
                    if !should_continue {
                        break;
                    }
                }
            }
        }

        let result = match last_error {
            Some(error) if attempts.len() > 1 => Err(RuntimeError::fallback_exhausted(error)),
            Some(error) => Err(error),
            None => Err(RuntimeError::target_resolution(
                "no target providers were resolved for this request",
            )),
        };

        if let Err(error) = &result {
            let terminal_error = terminal_failure_error(error);
            let terminal_provider = terminal_error
                .provider
                .or_else(|| attempts.last().map(|attempt| attempt.provider));
            let terminal_observer = terminal_provider
                .and_then(|provider| self.clients.get(&provider))
                .and_then(|client| {
                    resolve_observer_for_request(
                        client.runtime.observer.as_ref(),
                        self.observer.as_ref(),
                        options.observer.as_ref(),
                    )
                });
            let terminal_index = attempts.len().checked_sub(1);
            let request_end_event = RequestEndEvent {
                request_id: terminal_error.request_id.clone(),
                provider: terminal_provider,
                model: attempts.last().map(|attempt| attempt.model.clone()),
                target_index: terminal_index,
                attempt_index: terminal_index,
                elapsed: request_started_at.elapsed(),
                status_code: terminal_error.status_code,
                error_kind: Some(terminal_error.kind),
                error_message: Some(terminal_error.message.clone()),
            };
            safe_call_observer(terminal_observer, |runtime_observer| {
                runtime_observer.on_request_end(&request_end_event);
            });
        }

        result
    }

    pub fn resolve_targets(&self, options: &SendOptions) -> Result<Vec<Target>, RuntimeError> {
        let mut targets = Vec::new();

        if let Some(primary_target) = &options.target {
            if !targets.contains(primary_target) {
                targets.push(primary_target.clone());
            }

            if let Some(fallback_policy) = &options.fallback_policy {
                for target in &fallback_policy.targets {
                    if !targets.contains(target) {
                        targets.push(target.clone());
                    }
                }
            }
        } else if let Some(fallback_policy) = &options.fallback_policy {
            if fallback_policy.targets.is_empty() {
                return Err(RuntimeError::target_resolution(
                    "fallback policy requires at least one target",
                ));
            }
            for target in &fallback_policy.targets {
                if !targets.contains(target) {
                    targets.push(target.clone());
                }
            }
        } else {
            return Err(RuntimeError::target_resolution(
                "explicit target is required unless a fallback policy is provided",
            ));
        }

        for target in &targets {
            if !self.clients.contains_key(&target.provider) {
                return Err(RuntimeError::target_resolution(format!(
                    "provider {:?} is not registered",
                    target.provider
                )));
            }
        }

        Ok(targets)
    }
}

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
        let AgentToolkitBuilder {
            openai,
            anthropic,
            openrouter,
            observer,
        } = self;
        let mut clients = HashMap::new();

        if let Some(config) = openai {
            let mut runtime_builder = BaseClientBuilder::from_provider_config(config);
            runtime_builder.observer = observer.clone();
            let runtime = runtime_builder.build_runtime(ProviderId::OpenAi)?;
            clients.insert(ProviderId::OpenAi, ProviderClient::new(runtime));
        }
        if let Some(config) = anthropic {
            let mut runtime_builder = BaseClientBuilder::from_provider_config(config);
            runtime_builder.observer = observer.clone();
            let runtime = runtime_builder.build_runtime(ProviderId::Anthropic)?;
            clients.insert(ProviderId::Anthropic, ProviderClient::new(runtime));
        }
        if let Some(config) = openrouter {
            let mut runtime_builder = BaseClientBuilder::from_provider_config(config);
            runtime_builder.observer = observer.clone();
            let runtime = runtime_builder.build_runtime(ProviderId::OpenRouter)?;
            clients.insert(ProviderId::OpenRouter, ProviderClient::new(runtime));
        }

        if clients.is_empty() {
            return Err(RuntimeError::configuration(
                "at least one provider must be configured",
            ));
        }

        Ok(AgentToolkit { clients, observer })
    }
}

fn event_model(target_model: Option<&str>, request_model: &str) -> Option<String> {
    target_model
        .and_then(trimmed_non_empty)
        .or_else(|| trimmed_non_empty(request_model))
        .map(ToString::to_string)
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn terminal_failure_error(error: &RuntimeError) -> &RuntimeError {
    if error.kind == RuntimeErrorKind::FallbackExhausted
        && let Some(source) = error.source_ref()
        && let Some(terminal_error) = source.downcast_ref::<RuntimeError>()
    {
        return terminal_error;
    }
    error
}
