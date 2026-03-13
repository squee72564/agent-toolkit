use agent_core::{AdapterContext, ProviderId};

use crate::planner::ExecutionPlan;
use crate::runtime_error::RuntimeError;
use crate::types::AttemptMeta;

pub(super) struct PreparedAttempt {
    pub(super) selected_model: String,
    pub(super) adapter_context: AdapterContext,
}

pub(super) fn prepare_attempt(execution_plan: &ExecutionPlan) -> PreparedAttempt {
    PreparedAttempt {
        selected_model: execution_plan.attempt.model.clone(),
        adapter_context: execution_plan.adapter_context(),
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
