use agent_core::{
    AuthCredentials, ExecutionPlan, ProviderCapabilities, ProviderFamilyId, ProviderInstanceId,
    ProviderKind, ResolvedAuthContext, ResolvedProviderAttempt, ResolvedTransportOptions,
    ResponseMode, TaskRequest, TransportTimeoutOverrides,
};

use crate::agent_toolkit::AgentToolkit;
use crate::attempt_spec::AttemptSpec;
use crate::execution_options::ExecutionOptions;
use crate::planning_rejection_policy::PlanningRejectionPolicy;
use crate::provider_client::ProviderClient;
use crate::route::Route;
use crate::runtime_error::RuntimeError;
use crate::types::{
    AttemptDisposition, AttemptRecord, RoutePlanningFailure, RoutePlanningFailureReason, SkipReason,
};

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
    pub(crate) provider_instance: ProviderInstanceId,
    pub(crate) provider_kind: ProviderKind,
    pub(crate) model: String,
}

#[derive(Debug, Clone)]
pub(crate) enum AttemptPlanningError {
    Fatal(Box<RuntimeError>),
    Rejected(Box<PlanningRejection>),
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedRoutedAttempt {
    pub(crate) execution_plan: ExecutionPlan,
    pub(crate) target_index: usize,
}

#[derive(Debug, Clone)]
pub(crate) enum RoutedPlanningResult {
    Executable(Box<PlannedRoutedAttempt>),
    PlanningFailure(RoutePlanningFailure),
    Fatal(RuntimeError),
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

/// Outcome of planning a single fallback attempt during routed execution.
#[derive(Debug)]
pub(crate) enum FallbackPlanResult {
    /// Planning succeeded; the attempt is ready for execution.
    Executable(Box<ExecutionPlan>),
    /// Planning was rejected but the policy allows skipping to the next target.
    Skip,
    /// Planning failed or was rejected and no more targets should be tried.
    Stop,
}

/// Plans a single fallback attempt using the same validation and rejection
/// policy as pre-execution planning.  Used by both the non-streaming and
/// streaming fallback loops so that routed-attempt progression has one
/// unified code path.
pub(crate) fn plan_fallback_attempt(
    client: &ProviderClient,
    attempt_spec: &AttemptSpec,
    task: &TaskRequest,
    execution: &ExecutionOptions,
    planning_rejection_policy: PlanningRejectionPolicy,
    index: usize,
    total_attempts: usize,
) -> FallbackPlanResult {
    match plan_routed_attempt(client, attempt_spec, task, execution) {
        Ok(execution_plan) => FallbackPlanResult::Executable(Box::new(execution_plan)),
        Err(AttemptPlanningError::Rejected(_rejection)) => {
            if should_skip_planning_rejection(planning_rejection_policy, index, total_attempts) {
                FallbackPlanResult::Skip
            } else {
                FallbackPlanResult::Stop
            }
        }
        Err(AttemptPlanningError::Fatal(_error)) => FallbackPlanResult::Stop,
    }
}

pub(crate) fn plan_routed_execution(
    toolkit: &AgentToolkit,
    attempts: &[AttemptSpec],
    task: &TaskRequest,
    execution: &ExecutionOptions,
    planning_rejection_policy: PlanningRejectionPolicy,
) -> RoutedPlanningResult {
    let mut skipped_history = Vec::new();

    for (index, attempt_spec) in attempts.iter().enumerate() {
        let target = &attempt_spec.target;
        let Some(client) = toolkit.clients.get(&target.instance) else {
            return RoutedPlanningResult::Fatal(RuntimeError::target_resolution(format!(
                "provider instance {} is not registered",
                target.instance
            )));
        };

        match plan_routed_attempt(client, attempt_spec, task, execution) {
            Ok(execution_plan) => {
                return RoutedPlanningResult::Executable(Box::new(PlannedRoutedAttempt {
                    execution_plan,
                    target_index: index,
                }));
            }
            Err(AttemptPlanningError::Rejected(rejection)) => {
                skipped_history.push(planning_rejection_attempt_record(&rejection, index, index));

                if should_skip_planning_rejection(planning_rejection_policy, index, attempts.len())
                {
                    continue;
                }

                return RoutedPlanningResult::PlanningFailure(RoutePlanningFailure {
                    reason: planning_failure_reason(&skipped_history),
                    attempts: skipped_history,
                });
            }
            Err(AttemptPlanningError::Fatal(error)) => {
                return RoutedPlanningResult::Fatal(*error);
            }
        }
    }

    if skipped_history.is_empty() {
        RoutedPlanningResult::Fatal(RuntimeError::target_resolution(
            "no target providers were resolved for this request",
        ))
    } else {
        RoutedPlanningResult::PlanningFailure(RoutePlanningFailure {
            reason: planning_failure_reason(&skipped_history),
            attempts: skipped_history,
        })
    }
}

pub(crate) fn plan_direct_attempt(
    client: &ProviderClient,
    task: &TaskRequest,
    model_override: Option<&str>,
    execution: &ExecutionOptions,
) -> Result<ExecutionPlan, RuntimeError> {
    let attempt = AttemptSpec::to(crate::Target {
        instance: client.runtime.instance_id.clone(),
        model: model_override.map(ToString::to_string),
    });

    match plan_attempt(client, &attempt, task, execution) {
        Ok(plan) => Ok(plan),
        Err(AttemptPlanningError::Fatal(error)) => Err(*error),
        Err(AttemptPlanningError::Rejected(rejection)) => Err(rejection.error),
    }
}

pub(crate) fn plan_routed_attempt(
    client: &ProviderClient,
    attempt: &AttemptSpec,
    task: &TaskRequest,
    execution: &ExecutionOptions,
) -> Result<ExecutionPlan, AttemptPlanningError> {
    plan_attempt(client, attempt, task, execution)
}

fn plan_attempt(
    client: &ProviderClient,
    attempt: &AttemptSpec,
    task: &TaskRequest,
    execution: &ExecutionOptions,
) -> Result<ExecutionPlan, AttemptPlanningError> {
    let descriptor = client.runtime.adapter.descriptor();
    let capabilities = descriptor.capabilities;

    let selected_model = resolve_model(
        attempt.target.model.as_deref(),
        client.runtime.registered.config.default_model.as_deref(),
    )
    .map_err(|error| AttemptPlanningError::Fatal(Box::new(error)))?;

    validate_attempt_spec(
        attempt,
        client.runtime.instance_id.clone(),
        client.runtime.kind,
        descriptor.family,
        capabilities,
        execution.response_mode,
        selected_model.as_str(),
    )
    .map_err(AttemptPlanningError::Rejected)?;

    let platform = client
        .runtime
        .registered
        .platform_config(descriptor)
        .map_err(|error| AttemptPlanningError::Fatal(Box::new(error)))?;

    let auth = ResolvedAuthContext {
        credentials: Some(AuthCredentials::Token(
            client.runtime.registered.config.api_key.clone(),
        )),
    };
    let transport = ResolvedTransportOptions {
        request_id_header_override: execution.transport.request_id_header_override.clone(),
        route_extra_headers: execution.transport.extra_headers.clone(),
        attempt_extra_headers: attempt.execution.extra_headers.clone(),
        timeouts: resolve_transport_timeouts(client, attempt),
        retry_policy: resolve_retry_policy(client),
    };
    let resolved_attempt = ResolvedProviderAttempt {
        instance_id: client.runtime.instance_id.clone(),
        provider_kind: client.runtime.kind,
        family: descriptor.family,
        model: selected_model.clone(),
        capabilities,
        native_options: attempt.execution.native.clone(),
    };

    let execution_plan = ExecutionPlan {
        response_mode: execution.response_mode,
        task: task.clone(),
        provider_attempt: resolved_attempt,
        platform,
        auth,
        transport,
        capabilities,
    };

    client
        .runtime
        .adapter
        .plan_request(&execution_plan)
        .map_err(RuntimeError::from_adapter)
        .map_err(|error| {
            AttemptPlanningError::Rejected(Box::new(PlanningRejection {
                kind: PlanningRejectionKind::AdapterPlanningRejected,
                error,
                provider_instance: client.runtime.instance_id.clone(),
                provider_kind: client.runtime.kind,
                model: selected_model,
            }))
        })?;

    Ok(execution_plan)
}

fn validate_attempt_spec(
    attempt: &AttemptSpec,
    provider_instance: ProviderInstanceId,
    provider_kind: ProviderKind,
    provider_family: ProviderFamilyId,
    capabilities: ProviderCapabilities,
    response_mode: ResponseMode,
    selected_model: &str,
) -> Result<(), Box<PlanningRejection>> {
    if response_mode == ResponseMode::Streaming && !capabilities.supports_streaming {
        return Err(static_incompatibility(
            provider_instance,
            provider_kind,
            selected_model,
            format!(
                "provider {:?} does not support streaming responses",
                provider_kind,
            ),
        ));
    }

    let Some(native) = attempt.execution.native.as_ref() else {
        return Ok(());
    };

    if let Some(family) = native.family.as_ref()
        && family.family_id() != provider_family
    {
        return Err(static_incompatibility(
            provider_instance.clone(),
            provider_kind,
            selected_model,
            format!(
                "attempt native family options target {:?}, but route target {} resolves to family {:?}",
                family.family_id(),
                attempt.target.instance,
                provider_family,
            ),
        ));
    }

    if let Some(provider) = native.provider.as_ref()
        && provider.provider_kind() != provider_kind
    {
        return Err(static_incompatibility(
            provider_instance.clone(),
            provider_kind,
            selected_model,
            format!(
                "attempt native provider options target {:?}, but route target {} resolves to provider {:?}",
                provider.provider_kind(),
                attempt.target.instance,
                provider_kind,
            ),
        ));
    }

    if native.family.is_some() && !capabilities.supports_family_native_options {
        return Err(static_incompatibility(
            provider_instance.clone(),
            provider_kind,
            selected_model,
            format!(
                "provider {:?} does not support family-scoped native options",
                provider_kind,
            ),
        ));
    }

    if native.provider.is_some() && !capabilities.supports_provider_native_options {
        return Err(static_incompatibility(
            provider_instance,
            provider_kind,
            selected_model,
            format!(
                "provider {:?} does not support provider-scoped native options",
                provider_kind,
            ),
        ));
    }

    Ok(())
}

fn resolve_model(
    target_model: Option<&str>,
    provider_default_model: Option<&str>,
) -> Result<String, RuntimeError> {
    if let Some(model) = target_model.and_then(trimmed_non_empty) {
        return Ok(model.to_string());
    }

    if let Some(default_model) = provider_default_model.and_then(trimmed_non_empty) {
        return Ok(default_model.to_string());
    }

    Err(RuntimeError::configuration(
        "no model available for this attempt; set Target.model or configure ProviderConfig.default_model",
    ))
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn static_incompatibility(
    provider_instance: ProviderInstanceId,
    provider_kind: ProviderKind,
    model: impl Into<String>,
    message: impl Into<String>,
) -> Box<PlanningRejection> {
    let error = RuntimeError::configuration(message);

    Box::new(PlanningRejection {
        kind: PlanningRejectionKind::StaticIncompatibility,
        provider_instance,
        provider_kind,
        model: model.into(),
        error,
    })
}

fn planning_rejection_attempt_record(
    rejection: &PlanningRejection,
    target_index: usize,
    attempt_index: usize,
) -> AttemptRecord {
    AttemptRecord {
        provider_instance: rejection.provider_instance.clone(),
        provider_kind: rejection.provider_kind,
        model: rejection.model.clone(),
        target_index,
        attempt_index,
        disposition: AttemptDisposition::Skipped {
            reason: match rejection.kind {
                PlanningRejectionKind::StaticIncompatibility => SkipReason::StaticIncompatibility {
                    message: rejection.error.message.clone(),
                },
                PlanningRejectionKind::AdapterPlanningRejected => {
                    SkipReason::AdapterPlanningRejected {
                        message: rejection.error.message.clone(),
                    }
                }
            },
        },
    }
}

fn planning_failure_reason(skipped_history: &[AttemptRecord]) -> RoutePlanningFailureReason {
    if skipped_history.iter().any(|record| {
        matches!(
            record.disposition,
            AttemptDisposition::Skipped {
                reason: SkipReason::AdapterPlanningRejected { .. }
            }
        )
    }) {
        RoutePlanningFailureReason::AllAttemptsRejectedDuringPlanning
    } else {
        RoutePlanningFailureReason::NoCompatibleAttempts
    }
}

fn resolve_transport_timeouts(
    client: &ProviderClient,
    attempt: &AttemptSpec,
) -> TransportTimeoutOverrides {
    let config = &client.runtime.registered.config;
    let defaults = TransportTimeoutOverrides {
        request_timeout: config
            .request_timeout
            .or(Some(client.runtime.transport.request_timeout())),
        stream_setup_timeout: config
            .stream_timeout
            .or(Some(client.runtime.transport.stream_timeout())),
        stream_idle_timeout: config
            .stream_timeout
            .or(Some(client.runtime.transport.stream_timeout())),
    };

    let overrides = &attempt.execution.timeout_overrides;
    TransportTimeoutOverrides {
        request_timeout: overrides.request_timeout.or(defaults.request_timeout),
        stream_setup_timeout: overrides
            .stream_setup_timeout
            .or(defaults.stream_setup_timeout),
        stream_idle_timeout: overrides
            .stream_idle_timeout
            .or(defaults.stream_idle_timeout),
    }
}

fn resolve_retry_policy(client: &ProviderClient) -> agent_core::RetryPolicy {
    client
        .runtime
        .registered
        .config
        .retry_policy
        .clone()
        .unwrap_or_else(|| client.runtime.transport.retry_policy().clone())
}
