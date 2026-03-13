use std::collections::BTreeMap;
use std::time::Duration;

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
    /// Attempt-local timeout overrides.
    pub timeout_overrides: TransportTimeoutOverrides,
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
