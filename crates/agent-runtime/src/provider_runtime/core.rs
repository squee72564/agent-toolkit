use std::sync::Arc;

use agent_core::{
    CanonicalStreamEnvelope, ExecutionPlan, PlatformConfig, ProviderInstanceId, ProviderKind,
    Response, ResponseFormat, RuntimeWarning,
};
use agent_providers::{
    AdapterError, AdapterOperation, ProviderAdapterHandle, ProviderStreamProjectorHandle,
};
use agent_transport::{HttpJsonResponse, HttpSseResponse, HttpTransport, TransportResponseFraming};

use crate::RuntimeErrorKind;
use crate::observability::RuntimeObserver;
use crate::provider::RegisteredProvider;
use crate::provider_stream_runtime::{ProviderStreamRuntime, StreamRuntimeError};
use crate::runtime_error::RuntimeError;

use super::attempt::{PreparedAttempt, prepare_attempt};
use super::transport::{
    execute_planned_non_streaming, open_planned_stream, plan_execution, validate_streaming_plan,
};

#[derive(Clone)]
pub(crate) struct ProviderRuntime {
    pub(crate) instance_id: ProviderInstanceId,
    pub(crate) kind: ProviderKind,
    pub(crate) registered: RegisteredProvider,
    pub(crate) adapter: ProviderAdapterHandle,
    pub(crate) platform: PlatformConfig,
    pub(crate) transport: HttpTransport,
    pub(crate) observer: Option<Arc<dyn RuntimeObserver>>,
}

impl std::fmt::Debug for ProviderRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRuntime")
            .field("instance_id", &self.instance_id)
            .field("kind", &self.kind)
            .field("registered", &self.registered)
            .field("platform", &self.platform)
            .field("transport", &self.transport)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
}

pub(crate) enum ProviderAttemptOutcome {
    Success {
        response: Response,
        selected_model: String,
        status_code: Option<u16>,
        request_id: Option<String>,
    },
    Failure {
        error: RuntimeError,
        selected_model: String,
    },
}

pub(crate) enum ProviderStreamAttemptOutcome {
    Opened {
        stream: Box<OpenedProviderStream>,
        selected_model: String,
        status_code: Option<u16>,
        request_id: Option<String>,
    },
    Failure {
        error: RuntimeError,
        selected_model: String,
    },
}

pub(crate) struct OpenedProviderStream {
    pub(super) provider: ProviderKind,
    pub(super) response: HttpSseResponse,
    pub(super) response_format: ResponseFormat,
    pub(super) prepended_warnings: Vec<RuntimeWarning>,
    pub(super) projector: ProviderStreamProjectorHandle,
    pub(super) runtime: ProviderStreamRuntime,
    pub(super) transcript: Vec<CanonicalStreamEnvelope>,
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
                &mut self.projector,
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
            &mut self.projector,
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
        execution_plan: ExecutionPlan,
    ) -> ProviderAttemptOutcome {
        let PreparedAttempt { selected_model } = prepare_attempt(&execution_plan);
        let planned = match plan_execution(self, &execution_plan) {
            Ok(planned) => planned,
            Err(error) => {
                return ProviderAttemptOutcome::Failure {
                    error,
                    selected_model,
                };
            }
        };
        let provider_response = execute_planned_non_streaming(self, planned).await;

        match provider_response {
            Ok((response, http_response)) => ProviderAttemptOutcome::Success {
                response,
                selected_model,
                status_code: Some(http_response.head.status.as_u16()),
                request_id: http_response.head.request_id.clone(),
            },
            Err(error) => ProviderAttemptOutcome::Failure {
                error,
                selected_model,
            },
        }
    }

    pub(crate) async fn open_stream_attempt(
        &self,
        execution_plan: ExecutionPlan,
    ) -> ProviderStreamAttemptOutcome {
        let PreparedAttempt { selected_model } = prepare_attempt(&execution_plan);

        let planned = match plan_execution(self, &execution_plan) {
            Ok(planned) => planned,
            Err(error) => {
                return ProviderStreamAttemptOutcome::Failure {
                    error,
                    selected_model,
                };
            }
        };
        let stream = match validate_streaming_plan(self.kind, &planned.plan) {
            Ok(()) => open_planned_stream(self, planned).await,
            Err(error) => Err(error),
        };

        match stream {
            Ok(stream) => {
                let status_code = Some(stream.response.head.status.as_u16());
                let request_id = stream.response.head.request_id.clone();
                ProviderStreamAttemptOutcome::Opened {
                    stream: Box::new(stream),
                    selected_model,
                    status_code,
                    request_id,
                }
            }
            Err(error) => ProviderStreamAttemptOutcome::Failure {
                error,
                selected_model,
            },
        }
    }

    pub(crate) fn runtime_error_from_adapter(
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

pub(super) fn map_stream_runtime_error(
    provider: ProviderKind,
    error: StreamRuntimeError,
) -> RuntimeError {
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
    provider: ProviderKind,
    expected_mode: TransportResponseFraming,
    actual_response_kind: &'static str,
    head: &agent_transport::HttpResponseHead,
) -> RuntimeError {
    RuntimeError {
        kind: RuntimeErrorKind::ProtocolViolation,
        message: format!(
            "transport contract violated for {provider:?}: expected {} response, got {actual_response_kind}",
            expected_response_kind_label(expected_mode)
        ),
        provider: Some(provider),
        status_code: Some(head.status.as_u16()),
        request_id: head.request_id.clone(),
        provider_code: None,
        executed_failure_meta: None,
        source: None,
    }
}

pub(super) fn expected_response_kind_label(mode: TransportResponseFraming) -> &'static str {
    match mode {
        TransportResponseFraming::Json => "JSON",
        TransportResponseFraming::Sse => "SSE",
        TransportResponseFraming::Bytes => "bytes",
    }
}

pub(super) fn join_url(base_url: &str, endpoint_path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        endpoint_path.trim_start_matches('/')
    )
}

pub(super) fn extract_provider_code(body: &serde_json::Value) -> Option<String> {
    body.get("error")
        .and_then(serde_json::Value::as_object)
        .and_then(|error| error.get("code").or_else(|| error.get("type")))
        .and_then(value_to_string)
}

pub(super) fn value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) if !value.trim().is_empty() => {
            Some(value.trim().to_string())
        }
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn prepend_encode_warnings(
    response: &mut Response,
    mut encode_warnings: Vec<agent_core::types::RuntimeWarning>,
) {
    if encode_warnings.is_empty() {
        return;
    }
    encode_warnings.append(&mut response.warnings);
    response.warnings = encode_warnings;
}
