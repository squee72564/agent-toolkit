use crate::{RuntimeErrorKind, target::Target};

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
