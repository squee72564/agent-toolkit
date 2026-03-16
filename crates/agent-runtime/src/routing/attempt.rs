use crate::{RuntimeError, RuntimeErrorKind, routing::Target};

use std::collections::BTreeMap;

pub use agent_core::TransportTimeoutOverrides;

use agent_core::{NativeOptions, ProviderInstanceId, ProviderKind};

/// Attempt-local execution behavior that may vary by route target.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttemptExecutionOptions {
    /// Typed native request controls for the selected target.
    pub native: Option<NativeOptions>,
    /// Attempt-local timeout overrides.
    pub timeout_overrides: TransportTimeoutOverrides,
    /// Attempt-local extra outbound headers.
    pub extra_headers: BTreeMap<String, String>,
}

impl AttemptExecutionOptions {
    /// Replaces the native-option payload for this attempt.
    pub fn with_native_options(mut self, native: NativeOptions) -> Self {
        self.native = Some(native);
        self
    }

    /// Replaces the timeout overrides for this attempt.
    pub fn with_timeout_overrides(mut self, timeout_overrides: TransportTimeoutOverrides) -> Self {
        self.timeout_overrides = timeout_overrides;
        self
    }

    /// Replaces the attempt-local extra headers for this attempt.
    pub fn with_extra_headers(mut self, extra_headers: BTreeMap<String, String>) -> Self {
        self.extra_headers = extra_headers;
        self
    }
}

/// One candidate attempt in a route: destination plus target-local overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptSpec {
    /// Target selected for this attempt.
    pub target: Target,
    /// Attempt-local execution overrides for this target.
    pub execution: AttemptExecutionOptions,
}

impl AttemptSpec {
    /// Creates an attempt spec for the supplied target.
    pub fn to(target: Target) -> Self {
        Self {
            target,
            execution: AttemptExecutionOptions::default(),
        }
    }

    /// Replaces the attempt-local execution options.
    pub fn with_execution(mut self, execution: AttemptExecutionOptions) -> Self {
        self.execution = execution;
        self
    }

    /// Replaces the native options for this attempt.
    pub fn with_native_options(mut self, native: agent_core::NativeOptions) -> Self {
        self.execution = self.execution.with_native_options(native);
        self
    }

    /// Replaces the timeout overrides for this attempt.
    pub fn with_timeout_overrides(mut self, timeout_overrides: TransportTimeoutOverrides) -> Self {
        self.execution = self.execution.with_timeout_overrides(timeout_overrides);
        self
    }

    /// Replaces the attempt-local extra headers.
    pub fn with_extra_headers(
        mut self,
        extra_headers: std::collections::BTreeMap<String, String>,
    ) -> Self {
        self.execution = self.execution.with_extra_headers(extra_headers);
        self
    }
}

impl From<Target> for AttemptSpec {
    fn from(target: Target) -> Self {
        Self::to(target)
    }
}

/// Ordered route-attempt history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    pub provider_instance: ProviderInstanceId,
    pub provider_kind: ProviderKind,
    pub model: String,
    pub target_index: usize,
    pub attempt_index: usize,
    pub disposition: AttemptDisposition,
}

/// Planning-only reason for skipping a candidate route attempt before
/// provider execution begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    StaticIncompatibility { message: String },
    AdapterPlanningRejected { message: String },
}

/// Route-attempt disposition shared by planning-failure and execution-history
/// surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptDisposition {
    Skipped {
        reason: SkipReason,
    },
    Succeeded {
        status_code: Option<u16>,
        request_id: Option<String>,
    },
    Failed {
        error_kind: RuntimeErrorKind,
        error_message: String,
        status_code: Option<u16>,
        request_id: Option<String>,
    },
}

/// Planning-only route failure emitted when routing terminates before any
/// attempt executes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlanningFailure {
    pub reason: RoutePlanningFailureReason,
    pub attempts: Vec<AttemptRecord>,
}

/// Distinguishes pure static incompatibility exhaustion from adapter-planning
/// rejections that occurred before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePlanningFailureReason {
    NoCompatibleAttempts,
    AllAttemptsRejectedDuringPlanning,
}

impl std::fmt::Display for RoutePlanningFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.reason {
            RoutePlanningFailureReason::NoCompatibleAttempts => {
                "no compatible route attempts remained during planning"
            }
            RoutePlanningFailureReason::AllAttemptsRejectedDuringPlanning => {
                "all route attempts were rejected during planning"
            }
        };

        write!(f, "{message}")
    }
}

impl std::error::Error for RoutePlanningFailure {}

pub(crate) fn succeeded_attempt_record(
    provider_instance: ProviderInstanceId,
    provider_kind: ProviderKind,
    model: String,
    target_index: usize,
    attempt_index: usize,
    status_code: Option<u16>,
    request_id: Option<String>,
) -> AttemptRecord {
    AttemptRecord {
        provider_instance,
        provider_kind,
        model,
        target_index,
        attempt_index,
        disposition: AttemptDisposition::Succeeded {
            status_code,
            request_id,
        },
    }
}

pub(crate) fn failed_attempt_record(
    provider_instance: ProviderInstanceId,
    provider_kind: ProviderKind,
    model: String,
    target_index: usize,
    attempt_index: usize,
    error: &RuntimeError,
) -> AttemptRecord {
    AttemptRecord {
        provider_instance,
        provider_kind,
        model,
        target_index,
        attempt_index,
        disposition: AttemptDisposition::Failed {
            error_kind: error.kind,
            error_message: error.message.clone(),
            status_code: error.status_code,
            request_id: error.request_id.clone(),
        },
    }
}
