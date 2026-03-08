use agent_core::ProviderId;
use agent_providers::error::{AdapterError, AdapterErrorKind};
use agent_transport::{TimeoutStage, TransportError};
use std::error::Error as StdError;
use thiserror::Error;

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

#[derive(Debug, Error)]
#[error("{kind:?}: {message}")]
pub struct RuntimeError {
    pub kind: RuntimeErrorKind,
    pub message: String,
    pub provider: Option<ProviderId>,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub provider_code: Option<String>,
    #[source]
    pub source: Option<Box<dyn StdError + Send + Sync>>,
}

impl Clone for RuntimeError {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            message: self.message.clone(),
            provider: self.provider,
            status_code: self.status_code,
            request_id: self.request_id.clone(),
            provider_code: self.provider_code.clone(),
            source: None,
        }
    }
}

impl RuntimeError {
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

    pub fn fallback_exhausted(last_error: RuntimeError) -> Self {
        Self {
            kind: RuntimeErrorKind::FallbackExhausted,
            message: format!("fallback attempts exhausted: {}", last_error.message),
            provider: last_error.provider,
            status_code: last_error.status_code,
            request_id: last_error.request_id.clone(),
            provider_code: last_error.provider_code.clone(),
            source: Some(Box::new(last_error)),
        }
    }

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
            source: Some(Box::new(error)),
        }
    }

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
            source: Some(Box::new(error)),
        }
    }

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
