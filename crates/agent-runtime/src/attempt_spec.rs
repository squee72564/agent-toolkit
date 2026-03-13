use crate::attempt_execution_options::AttemptExecutionOptions;
use crate::target::Target;

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
    pub fn with_timeout_overrides(
        mut self,
        timeout_overrides: crate::attempt_execution_options::TransportTimeoutOverrides,
    ) -> Self {
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
