use std::collections::BTreeMap;
use std::time::Duration;

use agent_core::NativeOptions;

/// Optional caller-supplied timeout overrides for one attempt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransportTimeoutOverrides {
    /// Overrides the request timeout for this attempt when present.
    pub request_timeout: Option<Duration>,
    /// Overrides the stream-setup timeout for this attempt when present.
    pub stream_setup_timeout: Option<Duration>,
    /// Overrides the stream-idle timeout for this attempt when present.
    pub stream_idle_timeout: Option<Duration>,
}

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
