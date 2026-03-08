use agent_core::ProviderId;
use agent_providers::error::{AdapterError, AdapterErrorKind};
use agent_transport::TransportError;
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
        let (message, status_code) = match &error {
            TransportError::InvalidHeaderName => ("invalid header name".to_string(), None),
            TransportError::InvalidHeaderValue => ("invalid header value".to_string(), None),
            TransportError::Serialization => ("request serialization failed".to_string(), None),
            TransportError::Request(reqwest_error) => (
                reqwest_error.to_string(),
                reqwest_error.status().map(|status| status.as_u16()),
            ),
        };

        Self {
            kind: RuntimeErrorKind::Transport,
            message,
            provider: Some(provider),
            status_code,
            request_id: None,
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
