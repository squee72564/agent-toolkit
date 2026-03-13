use agent_core::{AdapterContext, AuthCredentials, ProviderId, Request};

use crate::attempt_execution_options::AttemptExecutionOptions;
use crate::execution_options::TransportOptions;
use crate::provider_runtime::ProviderRuntime;
use crate::runtime_error::RuntimeError;
use crate::types::AttemptMeta;

pub(super) struct PreparedAttempt {
    pub(super) request: Request,
    pub(super) selected_model: String,
    pub(super) adapter_context: AdapterContext,
}

pub(super) fn prepare_attempt(
    runtime: &ProviderRuntime,
    mut request: Request,
    model_override: Option<&str>,
    transport: &TransportOptions,
    execution: &AttemptExecutionOptions,
) -> Result<PreparedAttempt, Box<(RuntimeError, AttemptMeta)>> {
    let selected_model = match runtime.resolve_model(&request.model_id, model_override) {
        Ok(model) => model,
        Err(error) => {
            let meta = preflight_failure_meta(runtime.kind, &error);
            return Err(Box::new((error, meta)));
        }
    };
    request.model_id = selected_model.clone();

    Ok(PreparedAttempt {
        request,
        selected_model,
        adapter_context: AdapterContext {
            metadata: build_transport_metadata_shim(transport, execution),
            auth_token: Some(AuthCredentials::Token(
                runtime.registered.config.api_key.clone(),
            )),
        },
    })
}

/// REFACTOR-SHIM: temporary bridge that tunnels typed route/attempt transport
/// ownership through `AdapterContext.metadata` until phase 5 removes it.
pub(crate) fn build_transport_metadata_shim(
    transport: &TransportOptions,
    execution: &AttemptExecutionOptions,
) -> std::collections::BTreeMap<String, String> {
    let mut metadata = std::collections::BTreeMap::new();

    if let Some(request_id_header_override) = transport.request_id_header_override.as_ref() {
        metadata.insert(
            "transport.request_id_header".to_string(),
            request_id_header_override.clone(),
        );
    }

    for (key, value) in &transport.extra_headers {
        metadata.insert(normalize_transport_header_key(key), value.clone());
    }
    for (key, value) in &execution.extra_headers {
        metadata.insert(normalize_transport_header_key(key), value.clone());
    }

    metadata
}

fn normalize_transport_header_key(key: &str) -> String {
    if key.starts_with("transport.header.") {
        key.to_string()
    } else {
        format!("transport.header.{key}")
    }
}

pub(super) fn preflight_failure_meta(provider: ProviderId, error: &RuntimeError) -> AttemptMeta {
    AttemptMeta {
        provider,
        model: "<unset-model>".to_string(),
        success: false,
        status_code: None,
        request_id: None,
        error_kind: Some(error.kind),
        error_message: Some(error.message.clone()),
    }
}

pub(super) fn success_meta(
    provider: ProviderId,
    model: String,
    status_code: u16,
    request_id: Option<String>,
) -> AttemptMeta {
    AttemptMeta {
        provider,
        model,
        success: true,
        status_code: Some(status_code),
        request_id,
        error_kind: None,
        error_message: None,
    }
}

pub(super) fn failure_meta(
    provider: ProviderId,
    model: String,
    error: &RuntimeError,
) -> AttemptMeta {
    AttemptMeta {
        provider,
        model,
        success: false,
        status_code: error.status_code,
        request_id: error.request_id.clone(),
        error_kind: Some(error.kind),
        error_message: Some(error.message.clone()),
    }
}
