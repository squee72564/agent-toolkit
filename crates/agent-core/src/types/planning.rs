use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::StatusCode;

use super::identity::{ProviderCapabilities, ProviderFamilyId, ProviderInstanceId, ProviderKind};
use super::native_options::NativeOptions;
use super::platform::{AuthCredentials, PlatformConfig};
use super::task::TaskRequest;

/// Route-wide response delivery mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResponseMode {
    /// Return one completed canonical response.
    #[default]
    NonStreaming,
    /// Open a canonical event stream and finalize it separately.
    Streaming,
}

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

/// Retry settings applied before a response body is consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total number of attempts, including the initial request.
    pub max_attempts: u8,
    /// Base delay used for exponential backoff.
    pub initial_backoff: Duration,
    /// Maximum delay allowed for any retry backoff.
    pub max_backoff: Duration,
    /// HTTP statuses that should trigger another attempt when seen before body handling begins.
    pub retryable_status_codes: Vec<StatusCode>,
}

impl RetryPolicy {
    /// Returns `true` when `status_code` should trigger a retry.
    pub fn should_retry_status(&self, status_code: StatusCode) -> bool {
        self.retryable_status_codes.contains(&status_code)
    }

    /// Returns the backoff for the retry at `retry_index` using capped exponential growth.
    pub fn backoff_duration_for_retry(&self, retry_index: u8) -> Duration {
        let shift = u32::from(retry_index.min(31));
        let multiplier = 1_u32 << shift;
        self.initial_backoff
            .saturating_mul(multiplier)
            .min(self.max_backoff)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(2_000),
            retryable_status_codes: vec![
                StatusCode::REQUEST_TIMEOUT,
                StatusCode::TOO_MANY_REQUESTS,
                StatusCode::INTERNAL_SERVER_ERROR,
                StatusCode::BAD_GATEWAY,
                StatusCode::SERVICE_UNAVAILABLE,
                StatusCode::GATEWAY_TIMEOUT,
            ],
        }
    }
}

/// One concrete provider attempt selected during planning.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedProviderAttempt {
    /// Registered provider instance selected for this attempt.
    pub instance_id: ProviderInstanceId,
    /// Concrete provider adapter kind.
    pub provider_kind: ProviderKind,
    /// Shared protocol family for the selected provider.
    pub family: ProviderFamilyId,
    /// Fully resolved model for the attempt.
    pub model: String,
    /// Narrow static capability snapshot used during planning.
    pub capabilities: ProviderCapabilities,
    /// Typed target-scoped native options carried for this attempt.
    pub native_options: Option<NativeOptions>,
}

/// Authentication context resolved for one execution attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAuthContext {
    /// Optional credentials used by transport auth placement.
    pub credentials: Option<AuthCredentials>,
}

/// Typed transport controls resolved for one execution attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTransportOptions {
    /// Optional response request-id extraction override.
    pub request_id_header_override: Option<String>,
    /// Route-wide caller-owned extra headers.
    pub route_extra_headers: BTreeMap<String, String>,
    /// Attempt-local caller-owned extra headers.
    pub attempt_extra_headers: BTreeMap<String, String>,
    /// Resolved transport timeout selections for this attempt.
    pub timeouts: TransportTimeoutOverrides,
    /// Resolved intra-attempt retry policy for this attempt.
    pub retry_policy: RetryPolicy,
}

/// Fully resolved execution input handed to provider adapters and runtime.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Semantic response delivery mode for the logical call.
    pub response_mode: ResponseMode,
    /// Semantic task content.
    pub task: TaskRequest,
    /// Concrete provider attempt selected for execution.
    pub provider_attempt: ResolvedProviderAttempt,
    /// Transport-facing platform configuration resolved by runtime.
    pub platform: PlatformConfig,
    /// Authentication material resolved by runtime.
    pub auth: ResolvedAuthContext,
    /// Typed transport controls resolved by runtime.
    pub transport: ResolvedTransportOptions,
    /// Narrow static capability snapshot used during planning.
    pub capabilities: ProviderCapabilities,
}
