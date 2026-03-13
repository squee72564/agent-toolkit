use std::sync::Arc;
use std::time::Instant;

use agent_core::{Response, TaskRequest};

use crate::AgentToolkit;
use crate::attempt_spec::AttemptSpec;
use crate::execution_options::ExecutionOptions;
use crate::fallback::FallbackPolicy;
use crate::observer::RuntimeObserver;
use crate::planning_rejection_policy::PlanningRejectionPolicy;
use crate::provider_runtime::OpenedProviderStream;
use crate::runtime_error::RuntimeError;
use crate::types::{AttemptMeta, ResponseMeta, response_meta};

pub(super) struct StreamDriverState {
    pub(super) request_started_at: Instant,
    pub(super) request_observer: Option<Arc<dyn RuntimeObserver>>,
    pub(super) attempts: Vec<AttemptMeta>,
    pub(super) current_attempt: Option<LiveAttempt>,
    pub(super) routing: RoutingState,
    pub(super) first_envelope_emitted: bool,
    pub(super) pending_completion: Option<PendingCompletion>,
    pub(super) terminal_error: Option<RuntimeError>,
    pub(super) terminal_error_delivered: bool,
}

impl StreamDriverState {
    pub(super) fn new_direct(
        request_started_at: Instant,
        request_observer: Option<Arc<dyn RuntimeObserver>>,
        attempt: LiveAttempt,
    ) -> Self {
        Self {
            request_started_at,
            request_observer,
            attempts: Vec::new(),
            current_attempt: Some(attempt),
            routing: RoutingState::Direct,
            first_envelope_emitted: false,
            pending_completion: None,
            terminal_error: None,
            terminal_error_delivered: false,
        }
    }

    pub(super) fn new_routed(init: RoutedStreamInit<'_>) -> Self {
        Self {
            request_started_at: init.request_started_at,
            request_observer: init.request_observer,
            attempts: Vec::new(),
            current_attempt: Some(init.current_attempt),
            routing: RoutingState::Routed(Box::new(RoutedState {
                toolkit: init.toolkit.clone(),
                task: init.task,
                execution: init.execution,
                fallback_policy: init.fallback_policy,
                planning_rejection_policy: init.planning_rejection_policy,
                attempts: init.attempts,
                next_target_index: init.next_target_index,
            })),
            first_envelope_emitted: false,
            pending_completion: None,
            terminal_error: None,
            terminal_error_delivered: false,
        }
    }
}

pub(super) enum RoutingState {
    Direct,
    Routed(Box<RoutedState>),
}

pub(super) struct RoutedState {
    pub(super) toolkit: AgentToolkit,
    pub(super) task: TaskRequest,
    pub(super) execution: ExecutionOptions,
    pub(super) fallback_policy: FallbackPolicy,
    pub(super) planning_rejection_policy: PlanningRejectionPolicy,
    pub(super) attempts: Vec<AttemptSpec>,
    pub(super) next_target_index: usize,
}

pub(crate) struct RoutedStreamInit<'a> {
    pub(crate) task: TaskRequest,
    pub(crate) toolkit: &'a AgentToolkit,
    pub(crate) execution: ExecutionOptions,
    pub(crate) fallback_policy: FallbackPolicy,
    pub(crate) planning_rejection_policy: PlanningRejectionPolicy,
    pub(crate) request_started_at: Instant,
    pub(crate) request_observer: Option<Arc<dyn RuntimeObserver>>,
    pub(crate) attempts: Vec<AttemptSpec>,
    pub(crate) current_attempt: LiveAttempt,
    pub(crate) next_target_index: usize,
}

pub(super) struct PendingCompletion {
    pub(super) response: Response,
    pub(super) attempt: CompletedAttemptContext,
    pub(super) selected_provider: agent_core::ProviderId,
    pub(super) selected_model: String,
    pub(super) status_code: Option<u16>,
    pub(super) request_id: Option<String>,
}

impl PendingCompletion {
    pub(super) fn meta(self, mut attempts: Vec<AttemptMeta>) -> ResponseMeta {
        attempts.push(self.attempt.meta.clone());
        response_meta(
            self.selected_provider,
            self.selected_model,
            self.status_code,
            self.request_id,
            attempts,
        )
    }
}

pub(crate) struct LiveAttempt {
    pub(crate) stream: OpenedProviderStream,
    pub(crate) context: AttemptContext,
}

pub(crate) struct AttemptContext {
    pub(crate) target_index: usize,
    pub(crate) attempt_index: usize,
    pub(crate) started_at: Instant,
    pub(crate) observer: Option<Arc<dyn RuntimeObserver>>,
    pub(crate) provider: agent_core::ProviderId,
    pub(crate) model: String,
    pub(crate) request_id: Option<String>,
    pub(crate) status_code: Option<u16>,
}

pub(super) struct CompletedAttemptContext {
    pub(super) target_index: usize,
    pub(super) attempt_index: usize,
    pub(super) started_at: Instant,
    pub(super) observer: Option<Arc<dyn RuntimeObserver>>,
    pub(super) meta: AttemptMeta,
}
