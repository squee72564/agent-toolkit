use std::{collections::HashMap, sync::Arc};

use agent_core::{ProviderId, Request, Response};

use crate::message_response_stream::{
    AttemptContext, LiveAttempt, MessageResponseStream, RoutedStreamInit,
};
use crate::observer::RuntimeObserver;
use crate::provider_client::ProviderClient;
use crate::provider_runtime::{ProviderAttemptOutcome, ProviderStreamAttemptOutcome};
use crate::routed_messages_api::RoutedMessagesApi;
use crate::routed_streaming_api::RoutedStreamingApi;
use crate::runtime_error::RuntimeError;
use crate::send_options::SendOptions;
use crate::target::Target;
use crate::types::ResponseMeta;

mod builder;
mod execution;

pub use self::builder::AgentToolkitBuilder;
use self::execution::PreparedExecution;

#[derive(Clone)]
pub struct AgentToolkit {
    pub(crate) clients: HashMap<ProviderId, ProviderClient>,
    pub(crate) observer: Option<Arc<dyn RuntimeObserver>>,
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

    pub fn messages(&self) -> RoutedMessagesApi<'_> {
        RoutedMessagesApi::new(self)
    }

    pub fn streaming(&self) -> RoutedStreamingApi<'_> {
        RoutedStreamingApi::new(self)
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
        let prepared = PreparedExecution::new(self, &request, &options)?;
        prepared.emit_request_start(&request);

        let fallback_policy = options.fallback_policy.clone();
        let mut attempts = Vec::new();
        let mut last_error: Option<RuntimeError> = None;

        let request_model_id = request.model_id.clone();
        let mut request = Some(request);

        for (index, target) in prepared.targets.iter().enumerate() {
            let Some(client) = self.clients.get(&target.provider) else {
                let error = RuntimeError::target_resolution(format!(
                    "provider {:?} is not registered",
                    target.provider
                ));
                prepared.emit_request_end_failure(
                    Some(target.provider),
                    execution::event_model(target.model.as_deref(), &request_model_id),
                    Some(index),
                    Some(index),
                    &error,
                );
                return Err(error);
            };
            let attempt_execution =
                prepared.attempt(self, &options, &request_model_id, target, index);

            let is_last = index + 1 >= prepared.targets.len();
            let Some(attempt_request) = (if is_last {
                request.take()
            } else {
                request.as_ref().cloned()
            }) else {
                let error = RuntimeError::target_resolution(
                    "request state was exhausted before completing fallback attempts",
                );
                prepared.emit_request_end_failure(
                    Some(target.provider),
                    execution::event_model(target.model.as_deref(), &request_model_id),
                    Some(index),
                    Some(index),
                    &error,
                );
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
                    attempt_execution.emit_success(&meta, index);

                    attempts.push(meta.clone());
                    let response_meta = attempt_execution.response_meta(attempts, meta);
                    let response_model = response_meta.selected_model.clone();
                    prepared.emit_request_end_success(
                        Some(response_meta.selected_provider),
                        Some(response_model),
                        Some(index),
                        Some(index),
                        response_meta.request_id.clone(),
                        response_meta.status_code,
                    );
                    return Ok((response, response_meta));
                }
                ProviderAttemptOutcome::Failure { error, meta } => {
                    attempt_execution.emit_failure(&meta, index);

                    attempts.push(meta);
                    let should_continue = index + 1 < prepared.targets.len()
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
            prepared.emit_terminal_request_end(self, &options, &attempts, error);
        }

        result
    }

    pub(crate) async fn create_stream(
        &self,
        mut input: crate::message_create_input::MessageCreateInput,
        options: SendOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        input.stream = true;
        let request = input.into_request_with_options(None, true)?;
        self.send_stream(request, options).await
    }

    pub(crate) async fn send_stream(
        &self,
        request: Request,
        options: SendOptions,
    ) -> Result<MessageResponseStream, RuntimeError> {
        if !request.stream {
            return Err(RuntimeError::configuration(
                "streaming().create_request(...) requires request.stream = true",
            ));
        }

        let prepared = PreparedExecution::new(self, &request, &options)?;
        prepared.emit_request_start(&request);

        let fallback_policy = options.fallback_policy.clone();

        for (index, target) in prepared.targets.iter().enumerate() {
            let Some(client) = self.clients.get(&target.provider) else {
                return Err(RuntimeError::target_resolution(format!(
                    "provider {:?} is not registered",
                    target.provider
                )));
            };
            let attempt_execution =
                prepared.attempt(self, &options, &request.model_id, target, index);

            match client
                .runtime
                .open_stream_attempt(
                    request.clone(),
                    target.model.as_deref(),
                    options.metadata.clone(),
                )
                .await
            {
                ProviderStreamAttemptOutcome::Opened { stream, meta } => {
                    return Ok(MessageResponseStream::new_routed(RoutedStreamInit {
                        request,
                        toolkit: self,
                        options,
                        request_started_at: prepared.request_started_at,
                        request_observer: prepared.request_observer.clone(),
                        targets: prepared.targets.clone(),
                        current_attempt: LiveAttempt {
                            stream: *stream,
                            context: AttemptContext {
                                target_index: index,
                                attempt_index: index,
                                started_at: attempt_execution.started_at,
                                observer: attempt_execution.observer,
                                provider: meta.provider,
                                model: meta.model,
                                request_id: meta.request_id,
                                status_code: meta.status_code,
                            },
                        },
                        next_target_index: index + 1,
                    }));
                }
                ProviderStreamAttemptOutcome::Failure { error, meta } => {
                    attempt_execution.emit_failure(&meta, index);
                    let should_continue = index + 1 < prepared.targets.len()
                        && fallback_policy
                            .as_ref()
                            .is_some_and(|policy| policy.should_fallback(&error));
                    if !should_continue {
                        prepared.emit_request_end_failure(
                            Some(meta.provider),
                            Some(meta.model),
                            Some(index),
                            Some(index),
                            &error,
                        );
                        return Err(error);
                    }
                }
            }
        }

        Err(RuntimeError::target_resolution(
            "no target providers were resolved for this request",
        ))
    }

    fn resolve_attempt_observer(
        &self,
        options: &SendOptions,
        provider: ProviderId,
    ) -> Option<Arc<dyn RuntimeObserver>> {
        self.clients.get(&provider).and_then(|client| {
            crate::observer::resolve_observer_for_request(
                client.runtime.observer.as_ref(),
                self.observer.as_ref(),
                options.observer.as_ref(),
            )
            .cloned()
        })
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
