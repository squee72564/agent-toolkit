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
use crate::types::{
    AttemptRecord, ExecutedFailureMeta, ResponseMeta, executed_failure_meta, failed_attempt_record,
    response_meta, succeeded_attempt_record,
};

pub(super) struct StreamDriverState {
    pub(super) request_started_at: Instant,
    pub(super) request_observer: Option<Arc<dyn RuntimeObserver>>,
    pub(super) attempt_history: Vec<AttemptRecord>,
    pub(super) current_attempt: Option<LiveAttempt>,
    pub(super) routing: RoutingState,
    pub(super) commit_state: StreamCommitState,
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
            attempt_history: Vec::new(),
            current_attempt: Some(attempt),
            routing: RoutingState::Direct,
            commit_state: StreamCommitState::FallbackEligible,
            pending_completion: None,
            terminal_error: None,
            terminal_error_delivered: false,
        }
    }

    pub(super) fn new_routed(init: RoutedStreamInit<'_>) -> Self {
        Self {
            request_started_at: init.request_started_at,
            request_observer: init.request_observer,
            attempt_history: init.attempt_history,
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
            commit_state: StreamCommitState::FallbackEligible,
            pending_completion: None,
            terminal_error: None,
            terminal_error_delivered: false,
        }
    }

    pub(super) fn note_emitted_envelope(&mut self, envelope: &agent_core::CanonicalStreamEnvelope) {
        if !envelope.canonical.is_empty() {
            self.commit_state = StreamCommitState::Committed;
        }
    }

    pub(super) fn can_fallback(&self) -> bool {
        matches!(self.commit_state, StreamCommitState::FallbackEligible)
    }

    pub(super) fn note_attempt_skipped(&mut self, record: AttemptRecord) {
        self.attempt_history.push(record);
    }

    pub(super) fn note_attempt_failure(&mut self, context: &AttemptContext, error: &RuntimeError) {
        self.attempt_history.push(failed_attempt_record(
            context.provider_instance.clone(),
            context.provider,
            context.model.clone(),
            context.target_index,
            context.attempt_index,
            error,
        ));
    }

    pub(super) fn note_attempt_success(
        &mut self,
        context: &AttemptContext,
        status_code: Option<u16>,
        request_id: Option<String>,
    ) {
        self.attempt_history.push(succeeded_attempt_record(
            context.provider_instance.clone(),
            context.provider,
            context.model.clone(),
            context.target_index,
            context.attempt_index,
            status_code,
            request_id,
        ));
    }

    pub(super) fn note_failed_open_attempt(
        &mut self,
        provider_instance: agent_core::ProviderInstanceId,
        provider: agent_core::ProviderKind,
        model: String,
        target_index: usize,
        attempt_index: usize,
        error: &RuntimeError,
    ) {
        self.attempt_history.push(failed_attempt_record(
            provider_instance,
            provider,
            model,
            target_index,
            attempt_index,
            error,
        ));
    }

    pub(super) fn executed_failure_meta(
        &self,
        context: &AttemptContext,
        error: &RuntimeError,
    ) -> ExecutedFailureMeta {
        executed_failure_meta(
            context.provider_instance.clone(),
            context.provider,
            context.model.clone(),
            error.status_code.or(context.status_code),
            error
                .request_id
                .clone()
                .or_else(|| context.request_id.clone()),
            self.attempt_history.clone(),
        )
    }

    pub(super) fn success_meta(
        &self,
        context: &AttemptContext,
        status_code: Option<u16>,
        request_id: Option<String>,
    ) -> StreamSuccessMeta {
        StreamSuccessMeta {
            selected_provider_instance: context.provider_instance.clone(),
            selected_provider_kind: context.provider,
            selected_model: context.model.clone(),
            status_code,
            request_id,
            attempts: self.attempt_history.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamCommitState {
    FallbackEligible,
    Committed,
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
    pub(crate) attempt_history: Vec<AttemptRecord>,
    pub(crate) current_attempt: LiveAttempt,
    pub(crate) next_target_index: usize,
}

pub(super) struct PendingCompletion {
    pub(super) response: Response,
    pub(super) attempt: CompletedAttemptContext,
    pub(super) success_meta: StreamSuccessMeta,
}

impl PendingCompletion {
    pub(super) fn meta(self) -> ResponseMeta {
        response_meta(
            self.success_meta.selected_provider_instance,
            self.success_meta.selected_provider_kind,
            self.success_meta.selected_model,
            self.success_meta.status_code,
            self.success_meta.request_id,
            self.success_meta.attempts,
        )
    }
}

pub(super) struct StreamSuccessMeta {
    pub(super) selected_provider_instance: agent_core::ProviderInstanceId,
    pub(super) selected_provider_kind: agent_core::ProviderKind,
    pub(super) selected_model: String,
    pub(super) status_code: Option<u16>,
    pub(super) request_id: Option<String>,
    pub(super) attempts: Vec<AttemptRecord>,
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
    pub(crate) provider_instance: agent_core::ProviderInstanceId,
    pub(crate) provider: agent_core::ProviderKind,
    pub(crate) model: String,
    pub(crate) request_id: Option<String>,
    pub(crate) status_code: Option<u16>,
}

pub(super) struct CompletedAttemptContext {
    pub(super) target_index: usize,
    pub(super) attempt_index: usize,
    pub(super) started_at: Instant,
    pub(super) observer: Option<Arc<dyn RuntimeObserver>>,
    pub(super) record: AttemptRecord,
}
