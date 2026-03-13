use std::collections::BTreeMap;

use agent_core::{AdapterContext, AuthCredentials, ProviderId, Request};

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
    metadata: BTreeMap<String, String>,
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
            metadata,
            auth_token: Some(AuthCredentials::Token(
                runtime.registered.config.api_key.clone(),
            )),
        },
    })
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
