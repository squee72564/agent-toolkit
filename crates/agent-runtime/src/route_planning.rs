use agent_core::{ProviderCapabilities, ProviderFamilyId, ProviderKind, Request};

use crate::agent_toolkit::AgentToolkit;
use crate::attempt_spec::AttemptSpec;
use crate::execution_options::ResponseMode;
use crate::provider_client::ProviderClient;
use crate::route::Route;
use crate::runtime_error::RuntimeError;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanningRejectionKind {
    StaticIncompatibility,
    AdapterPlanningRejected,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone)]
pub(crate) struct PlanningRejection {
    pub(crate) kind: PlanningRejectionKind,
    pub(crate) error: RuntimeError,
}

#[derive(Debug, Clone)]
pub(crate) enum PrepareAttemptError {
    Fatal(RuntimeError),
    Rejected(PlanningRejection),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedRouteAttempt {
    pub(crate) selected_model: String,
}

pub(crate) fn resolve_route_targets(
    toolkit: &AgentToolkit,
    route: &Route,
) -> Result<Vec<AttemptSpec>, RuntimeError> {
    let attempts = route.attempts();

    for attempt in &attempts {
        if !toolkit.clients.contains_key(&attempt.target.instance) {
            return Err(RuntimeError::target_resolution(format!(
                "provider instance {} is not registered",
                attempt.target.instance
            )));
        }
    }

    Ok(attempts)
}

pub(crate) fn should_skip_planning_rejection(
    policy: crate::PlanningRejectionPolicy,
    index: usize,
    attempt_count: usize,
) -> bool {
    policy == crate::PlanningRejectionPolicy::SkipRejectedTargets && index + 1 < attempt_count
}

pub(crate) fn prepare_route_attempt(
    client: &ProviderClient,
    attempt: &AttemptSpec,
    response_mode: ResponseMode,
    request: &Request,
) -> Result<PreparedRouteAttempt, PrepareAttemptError> {
    validate_attempt_spec(
        attempt,
        client.runtime.kind,
        client.runtime.adapter.descriptor().family,
        client.runtime.adapter.descriptor().capabilities,
        response_mode,
    )
    .map_err(PrepareAttemptError::Rejected)?;

    let selected_model = client
        .runtime
        .resolve_model(&request.model_id, attempt.target.model.as_deref())
        .map_err(PrepareAttemptError::Fatal)?;

    let mut planning_request = request.clone();
    planning_request.model_id = selected_model.clone();
    client
        .runtime
        .adapter
        .plan_request(planning_request, attempt.execution.native.as_ref())
        .map_err(RuntimeError::from_adapter)
        .map_err(|error| {
            PrepareAttemptError::Rejected(PlanningRejection {
                kind: PlanningRejectionKind::AdapterPlanningRejected,
                error,
            })
        })?;

    Ok(PreparedRouteAttempt { selected_model })
}

fn validate_attempt_spec(
    attempt: &AttemptSpec,
    provider_kind: ProviderKind,
    provider_family: ProviderFamilyId,
    capabilities: ProviderCapabilities,
    response_mode: ResponseMode,
) -> Result<(), PlanningRejection> {
    if response_mode == ResponseMode::Streaming && !capabilities.supports_streaming {
        return Err(static_incompatibility(format!(
            "provider {:?} does not support streaming responses",
            provider_kind,
        )));
    }

    let Some(native) = attempt.execution.native.as_ref() else {
        return Ok(());
    };

    if let Some(family) = native.family.as_ref()
        && family.family_id() != provider_family
    {
        return Err(static_incompatibility(format!(
            "attempt native family options target {:?}, but route target {} resolves to family {:?}",
            family.family_id(),
            attempt.target.instance,
            provider_family,
        )));
    }

    if let Some(provider) = native.provider.as_ref()
        && provider.provider_kind() != provider_kind
    {
        return Err(static_incompatibility(format!(
            "attempt native provider options target {:?}, but route target {} resolves to provider {:?}",
            provider.provider_kind(),
            attempt.target.instance,
            provider_kind,
        )));
    }

    if native.family.is_some() && !capabilities.supports_family_native_options {
        return Err(static_incompatibility(format!(
            "provider {:?} does not support family-scoped native options",
            provider_kind,
        )));
    }

    if native.provider.is_some() && !capabilities.supports_provider_native_options {
        return Err(static_incompatibility(format!(
            "provider {:?} does not support provider-scoped native options",
            provider_kind,
        )));
    }

    Ok(())
}

fn static_incompatibility(message: impl Into<String>) -> PlanningRejection {
    PlanningRejection {
        kind: PlanningRejectionKind::StaticIncompatibility,
        error: RuntimeError::configuration(message),
    }
}
