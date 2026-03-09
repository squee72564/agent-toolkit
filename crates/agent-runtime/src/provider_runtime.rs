use std::{collections::BTreeMap, sync::Arc};

use agent_core::{
    CanonicalStreamEnvelope, PlatformConfig, ProviderId, Request, Response, ResponseFormat,
    RuntimeWarning,
};
use agent_providers::error::AdapterOperation;
use agent_providers::{
    adapter::ProviderAdapter, error::AdapterError, streaming::ProviderStreamProjector,
};
use agent_transport::{HttpJsonResponse, HttpResponseMode, HttpSseResponse, HttpTransport};

use crate::observer::RuntimeObserver;
use crate::provider_stream_runtime::{ProviderStreamRuntime, StreamRuntimeError};
use crate::runtime_error::RuntimeError;
use crate::types::AttemptMeta;

mod attempt;
mod transport;

use self::attempt::{PreparedAttempt, prepare_attempt};
use self::transport::{
    execute_planned_non_streaming, open_planned_stream, plan_execution, validate_streaming_plan,
};

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

pub(crate) enum ProviderStreamAttemptOutcome {
    Opened {
        stream: Box<OpenedProviderStream>,
        meta: AttemptMeta,
    },
    Failure {
        error: RuntimeError,
        meta: AttemptMeta,
    },
}

pub(crate) struct OpenedProviderStream {
    provider: ProviderId,
    response: HttpSseResponse,
    response_format: ResponseFormat,
    prepended_warnings: Vec<RuntimeWarning>,
    projector: Box<dyn ProviderStreamProjector>,
    runtime: ProviderStreamRuntime,
    transcript: Vec<CanonicalStreamEnvelope>,
}

impl std::fmt::Debug for OpenedProviderStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenedProviderStream")
            .finish_non_exhaustive()
    }
}

impl OpenedProviderStream {
    pub(crate) async fn next_envelope(
        &mut self,
    ) -> Result<Option<CanonicalStreamEnvelope>, RuntimeError> {
        match self
            .runtime
            .next_envelope(
                &mut self.response,
                self.projector.as_mut(),
                AdapterOperation::ProjectStreamEvent,
            )
            .await
        {
            Ok(Some(envelope)) => {
                self.transcript.push(envelope.clone());
                Ok(Some(envelope))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(map_stream_runtime_error(self.provider, error)),
        }
    }

    pub(crate) fn finish(self) -> Result<(Response, HttpJsonResponse), RuntimeError> {
        let provider = self.provider;
        self.runtime_finalized()
            .map_err(|error| map_stream_runtime_error(provider, error))
    }

    fn runtime_finalized(mut self) -> Result<(Response, HttpJsonResponse), StreamRuntimeError> {
        self.runtime.finalize_response(
            self.response,
            self.projector.as_mut(),
            &self.response_format,
            self.prepended_warnings,
            self.transcript,
            AdapterOperation::FinalizeStream,
        )
    }
}

impl ProviderRuntime {
    pub(crate) async fn execute_attempt(
        &self,
        request: Request,
        model_override: Option<&str>,
        metadata: BTreeMap<String, String>,
    ) -> ProviderAttemptOutcome {
        let PreparedAttempt {
            request,
            selected_model,
            adapter_context,
        } = match prepare_attempt(self, request, model_override, metadata) {
            Ok(prepared) => prepared,
            Err(error_and_meta) => {
                let (error, meta) = *error_and_meta;
                return ProviderAttemptOutcome::Failure { error, meta };
            }
        };
        let provider_response = match plan_execution(self, request) {
            Ok(planned) => execute_planned_non_streaming(self, planned, &adapter_context).await,
            Err(error) => Err(error),
        };

        match provider_response {
            Ok((response, http_response)) => ProviderAttemptOutcome::Success {
                meta: attempt::success_meta(
                    self.provider,
                    selected_model,
                    http_response.head.status.as_u16(),
                    http_response.head.request_id.clone(),
                ),
                response,
            },
            Err(error) => ProviderAttemptOutcome::Failure {
                meta: attempt::failure_meta(self.provider, selected_model, &error),
                error,
            },
        }
    }

    pub(crate) async fn open_stream_attempt(
        &self,
        request: Request,
        model_override: Option<&str>,
        metadata: BTreeMap<String, String>,
    ) -> ProviderStreamAttemptOutcome {
        let PreparedAttempt {
            request,
            selected_model,
            adapter_context,
        } = match prepare_attempt(self, request, model_override, metadata) {
            Ok(prepared) => prepared,
            Err(error_and_meta) => {
                let (error, meta) = *error_and_meta;
                return ProviderStreamAttemptOutcome::Failure { error, meta };
            }
        };

        let stream = match plan_execution(self, request).and_then(|planned| {
            validate_streaming_plan(self.provider, &planned.plan)?;
            Ok(planned)
        }) {
            Ok(planned) => open_planned_stream(self, planned, &adapter_context).await,
            Err(error) => Err(error),
        };

        match stream {
            Ok(stream) => ProviderStreamAttemptOutcome::Opened {
                meta: attempt::success_meta(
                    self.provider,
                    selected_model,
                    stream.response.head.status.as_u16(),
                    stream.response.head.request_id.clone(),
                ),
                stream: Box::new(stream),
            },
            Err(error) => ProviderStreamAttemptOutcome::Failure {
                meta: attempt::failure_meta(self.provider, selected_model, &error),
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

fn map_stream_runtime_error(provider: ProviderId, error: StreamRuntimeError) -> RuntimeError {
    match error {
        StreamRuntimeError::Transport {
            error,
            request_id,
            status_code,
        } => {
            let mut runtime_error = RuntimeError::from_transport(provider, error);
            if runtime_error.request_id.is_none() {
                runtime_error.request_id = request_id;
            }
            if runtime_error.status_code.is_none() {
                runtime_error.status_code = status_code;
            }
            runtime_error
        }
        StreamRuntimeError::Adapter {
            mut error,
            request_id,
            status_code,
        } => {
            if error.request_id.is_none() {
                error.request_id = request_id;
            }
            if error.status_code.is_none() {
                error.status_code = status_code;
            }
            RuntimeError::from_adapter(error)
        }
    }
}

pub(crate) fn response_mode_mismatch_error(
    provider: ProviderId,
    expected_mode: HttpResponseMode,
    actual_response_kind: &'static str,
    head: &agent_transport::HttpResponseHead,
) -> RuntimeError {
    RuntimeError {
        kind: crate::RuntimeErrorKind::ProtocolViolation,
        message: format!(
            "transport contract violated for {provider:?}: expected {} response, got {actual_response_kind}",
            expected_response_kind_label(expected_mode)
        ),
        provider: Some(provider),
        status_code: Some(head.status.as_u16()),
        request_id: head.request_id.clone(),
        provider_code: None,
        source: None,
    }
}

fn expected_response_kind_label(mode: HttpResponseMode) -> &'static str {
    match mode {
        HttpResponseMode::Json => "JSON",
        HttpResponseMode::Sse => "SSE",
        HttpResponseMode::Bytes => "bytes",
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
