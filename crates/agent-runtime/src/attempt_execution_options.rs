use std::collections::BTreeMap;

pub use agent_core::TransportTimeoutOverrides;

use agent_core::NativeOptions;

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
