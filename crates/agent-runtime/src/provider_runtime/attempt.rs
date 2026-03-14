use agent_core::ExecutionPlan;

pub(super) struct PreparedAttempt {
    pub(super) selected_model: String,
}

pub(super) fn prepare_attempt(execution_plan: &ExecutionPlan) -> PreparedAttempt {
    PreparedAttempt {
        selected_model: execution_plan.provider_attempt.model.clone(),
    }
}
