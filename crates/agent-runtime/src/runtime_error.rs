use agent_core::ProviderId;
use agent_providers::error::{AdapterError, AdapterErrorKind};
use agent_transport::{TimeoutStage, TransportError};
use std::error::Error as StdError;
use std::sync::Arc;
use thiserror::Error;

/// Category of error surfaced by the runtime layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    Configuration,
    TargetResolution,
    FallbackExhausted,
    Validation,
    Encode,
    Decode,
    ProtocolViolation,
    UnsupportedFeature,
    Upstream,
    Transport,
}

/// Runtime-level error surfaced by `agent-runtime`.
///
/// `source` carries the underlying causal chain when one exists. In particular,
/// [`RuntimeErrorKind::FallbackExhausted`] wraps the terminal attempt error in
/// `source`, so callers that need the last concrete failure should inspect
/// [`RuntimeError::source_ref`] and downcast to `RuntimeError`.
#[derive(Debug, Error, Clone)]
#[error("{kind:?}: {message}")]
pub struct RuntimeError {
    /// High-level runtime error category.
    pub kind: RuntimeErrorKind,
    /// Human-readable error message.
    pub message: String,
    /// Provider associated with the error, when known.
    pub provider: Option<ProviderId>,
    /// HTTP status code associated with the error, when known.
    pub status_code: Option<u16>,
    /// Provider request identifier, when one was returned.
    pub request_id: Option<String>,
    /// Provider-specific error code, when available.
    pub provider_code: Option<String>,
    #[source]
    /// Underlying source error preserved for inspection and downcasting.
    pub source: Option<Arc<dyn StdError + Send + Sync>>,
}

impl RuntimeError {
    /// Creates a configuration error.
    pub fn configuration(message: impl Into<String>) -> Self {
        Self {
            kind: RuntimeErrorKind::Configuration,
            message: message.into(),
            provider: None,
            status_code: None,
            request_id: None,
            provider_code: None,
            source: None,
        }
    }

    /// Creates a target resolution error.
    pub fn target_resolution(message: impl Into<String>) -> Self {
        Self {
            kind: RuntimeErrorKind::TargetResolution,
            message: message.into(),
            provider: None,
            status_code: None,
            request_id: None,
            provider_code: None,
            source: None,
        }
    }

    /// Wraps the last error after all fallback targets have been exhausted.
    pub fn fallback_exhausted(last_error: RuntimeError) -> Self {
        Self {
            kind: RuntimeErrorKind::FallbackExhausted,
            message: format!("fallback attempts exhausted: {}", last_error.message),
            provider: last_error.provider,
            status_code: last_error.status_code,
            request_id: last_error.request_id.clone(),
            provider_code: last_error.provider_code.clone(),
            source: Some(Arc::new(last_error)),
        }
    }

    /// Converts a provider adapter error into a runtime error.
    pub fn from_adapter(error: AdapterError) -> Self {
        let status_code = error.status_code;
        let request_id = error.request_id.clone();
        let provider_code = error.provider_code.clone();
        let provider = error.provider;

        Self {
            kind: map_adapter_error_kind(error.kind),
            message: error.message.clone(),
            provider: Some(provider),
            status_code,
            request_id,
            provider_code,
            source: Some(Arc::new(error)),
        }
    }

    /// Converts a transport-layer error into a runtime error.
    pub fn from_transport(provider: ProviderId, error: TransportError) -> Self {
        let (message, status_code, request_id) = match &error {
            TransportError::InvalidHeaderName => ("invalid header name".to_string(), None, None),
            TransportError::InvalidHeaderValue => ("invalid header value".to_string(), None, None),
            TransportError::Serialization => {
                ("request serialization failed".to_string(), None, None)
            }
            TransportError::Timeout { stage } => (
                match stage {
                    TimeoutStage::Request => "request timed out".to_string(),
                    TimeoutStage::StreamSetup => "stream setup timed out".to_string(),
                    TimeoutStage::FirstByte => "stream first byte timed out".to_string(),
                    TimeoutStage::StreamIdle => "stream idle timed out".to_string(),
                },
                None,
                None,
            ),
            TransportError::Status { head } => (
                format!("upstream returned HTTP {}", head.status.as_u16()),
                Some(head.status.as_u16()),
                head.request_id.clone(),
            ),
            TransportError::ContentTypeMismatch {
                expected,
                actual,
                head,
            } => (
                format!(
                    "unexpected content type: expected {expected}, got {}",
                    actual.as_deref().unwrap_or("<missing>")
                ),
                Some(head.status.as_u16()),
                head.request_id.clone(),
            ),
            TransportError::StreamTerminated {
                reason,
                message,
                head,
            } => (
                format!("stream terminated unexpectedly ({reason}): {message}"),
                Some(head.status.as_u16()),
                head.request_id.clone(),
            ),
            TransportError::SseParse(message) => {
                (format!("invalid SSE stream: {message}"), None, None)
            }
            TransportError::SseLimit { kind, size, max } => {
                (format!("{kind} exceeded limit: {size} > {max}"), None, None)
            }
            TransportError::Request(reqwest_error) => (
                reqwest_error.to_string(),
                reqwest_error.status().map(|status| status.as_u16()),
                None,
            ),
        };

        Self {
            kind: RuntimeErrorKind::Transport,
            message,
            provider: Some(provider),
            status_code,
            request_id,
            provider_code: None,
            source: Some(Arc::new(error)),
        }
    }

    /// Returns the source error as a trait object for further inspection.
    pub fn source_ref(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

fn map_adapter_error_kind(kind: AdapterErrorKind) -> RuntimeErrorKind {
    match kind {
        AdapterErrorKind::Validation => RuntimeErrorKind::Validation,
        AdapterErrorKind::Encode => RuntimeErrorKind::Encode,
        AdapterErrorKind::Decode => RuntimeErrorKind::Decode,
        AdapterErrorKind::ProtocolViolation => RuntimeErrorKind::ProtocolViolation,
        AdapterErrorKind::UnsupportedFeature => RuntimeErrorKind::UnsupportedFeature,
        AdapterErrorKind::Upstream => RuntimeErrorKind::Upstream,
        AdapterErrorKind::Transport => RuntimeErrorKind::Transport,
    }
}
