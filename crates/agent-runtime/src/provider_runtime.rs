use std::{collections::BTreeMap, sync::Arc};

use agent_core::{AdapterContext, AuthCredentials, PlatformConfig, ProviderId, Request, Response};
use agent_providers::{adapter::ProviderAdapter, error::AdapterError};
use agent_transport::{HttpJsonResponse, HttpTransport};

use crate::observer::RuntimeObserver;
use crate::runtime_error::RuntimeError;
use crate::types::AttemptMeta;

#[derive(Clone)]
pub(crate) struct ProviderRuntime {
    pub(crate) provider: ProviderId,
    pub(crate) adapter: &'static dyn ProviderAdapter,
    pub(crate) platform: PlatformConfig,
    pub(crate) auth_token: String,
    pub(crate) default_model: Option<String>,
    pub(crate) transport: HttpTransport,
    pub(crate) observer: Option<Arc<dyn RuntimeObserver>>,
}

impl std::fmt::Debug for ProviderRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRuntime")
            .field("provider", &self.provider)
            .field("platform", &self.platform)
            .field("auth_token", &"<redacted>")
            .field("default_model", &self.default_model)
            .field("transport", &self.transport)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
}

pub(crate) enum ProviderAttemptOutcome {
    Success {
        response: Response,
        meta: AttemptMeta,
    },
    Failure {
        error: RuntimeError,
        meta: AttemptMeta,
    },
}

impl ProviderRuntime {
    pub(crate) async fn execute_attempt(
        &self,
        mut request: Request,
        model_override: Option<&str>,
        metadata: BTreeMap<String, String>,
    ) -> ProviderAttemptOutcome {
        let selected_model = match self.resolve_model(&request.model_id, model_override) {
            Ok(model) => model,
            Err(error) => {
                return ProviderAttemptOutcome::Failure {
                    meta: AttemptMeta {
                        provider: self.provider,
                        model: "<unset-model>".to_string(),
                        success: false,
                        status_code: None,
                        request_id: None,
                        error_kind: Some(error.kind),
                        error_message: Some(error.message.clone()),
                    },
                    error,
                };
            }
        };
        request.model_id = selected_model.clone();

        let adapter_context = AdapterContext {
            metadata,
            auth_token: Some(AuthCredentials::Token(self.auth_token.clone())),
        };
        let url = join_url(&self.platform.base_url, self.adapter.endpoint_path());

        let provider_response = self
            .execute_adapter_attempt(request, &url, &adapter_context)
            .await;

        match provider_response {
            Ok((response, http_response)) => ProviderAttemptOutcome::Success {
                meta: AttemptMeta {
                    provider: self.provider,
                    model: selected_model,
                    success: true,
                    status_code: Some(http_response.head.status.as_u16()),
                    request_id: http_response.head.request_id.clone(),
                    error_kind: None,
                    error_message: None,
                },
                response,
            },
            Err(error) => ProviderAttemptOutcome::Failure {
                meta: AttemptMeta {
                    provider: self.provider,
                    model: selected_model,
                    success: false,
                    status_code: error.status_code,
                    request_id: error.request_id.clone(),
                    error_kind: Some(error.kind),
                    error_message: Some(error.message.clone()),
                },
                error,
            },
        }
    }

    fn resolve_model(
        &self,
        request_model: &str,
        model_override: Option<&str>,
    ) -> Result<String, RuntimeError> {
        let trimmed_override = model_override.and_then(trimmed_non_empty);
        if let Some(model) = trimmed_override {
            return Ok(model.to_string());
        }

        if let Some(model) = trimmed_non_empty(request_model) {
            return Ok(model.to_string());
        }

        if let Some(default_model) = self.default_model.as_deref().and_then(trimmed_non_empty) {
            return Ok(default_model.to_string());
        }

        Err(RuntimeError::configuration(format!(
            "no model available for provider {:?}; set a default model or pass one per request",
            self.provider
        )))
    }

    async fn execute_adapter_attempt(
        &self,
        request: Request,
        url: &str,
        adapter_context: &AdapterContext,
    ) -> Result<(Response, HttpJsonResponse), RuntimeError> {
        let response_format = request.response_format.clone();
        let encoded = self
            .adapter
            .encode_request(request)
            .map_err(RuntimeError::from_adapter)?;
        let mut provider_response = self
            .transport
            .post_json_value(&self.platform, url, &encoded.body, adapter_context)
            .await
            .map_err(|error| RuntimeError::from_transport(self.provider, error))?;
        let provider_code = extract_provider_code(&provider_response.body);
        let response_body = std::mem::replace(&mut provider_response.body, serde_json::Value::Null);
        let mut response = self
            .adapter
            .decode_response(response_body, &response_format)
            .map_err(|mut error| {
                if error.provider_code.is_none() {
                    error.provider_code = provider_code;
                }
                self.runtime_error_from_adapter(error, Some(&provider_response))
            })?;
        prepend_encode_warnings(&mut response, encoded.warnings);
        Ok((response, provider_response))
    }

    fn runtime_error_from_adapter(
        &self,
        mut adapter_error: AdapterError,
        response: Option<&HttpJsonResponse>,
    ) -> RuntimeError {
        if let Some(response) = response {
            if adapter_error.status_code.is_none() {
                adapter_error.status_code = Some(response.head.status.as_u16());
            }
            if adapter_error.request_id.is_none() {
                adapter_error.request_id = response.head.request_id.clone();
            }
            if adapter_error.provider_code.is_none() {
                adapter_error.provider_code = extract_provider_code(&response.body);
            }
        }
        RuntimeError::from_adapter(adapter_error)
    }
}

fn join_url(base_url: &str, endpoint_path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        endpoint_path.trim_start_matches('/')
    )
}

fn extract_provider_code(body: &serde_json::Value) -> Option<String> {
    body.get("error")
        .and_then(serde_json::Value::as_object)
        .and_then(|error| error.get("code").or_else(|| error.get("type")))
        .and_then(value_to_string)
}

fn value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) if !value.trim().is_empty() => {
            Some(value.trim().to_string())
        }
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn prepend_encode_warnings(
    response: &mut Response,
    mut encode_warnings: Vec<agent_core::types::RuntimeWarning>,
) {
    if encode_warnings.is_empty() {
        return;
    }
    encode_warnings.append(&mut response.warnings);
    response.warnings = encode_warnings;
}
