use std::error::Error as StdError;

use agent_core::types::ProviderId;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterOperation {
    EncodeRequest,
    DecodeResponse,
    BuildHttpRequest,
    ParseHttpResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterErrorKind {
    Validation,
    Encode,
    Decode,
    ProtocolViolation,
    UnsupportedFeature,
    Upstream,
    Transport,
}

#[derive(Debug, Error)]
#[error("{provider:?}::{operation:?}::{kind:?}: {message}")]
pub struct AdapterError {
    pub kind: AdapterErrorKind,
    pub provider: ProviderId,
    pub operation: AdapterOperation,
    pub message: String,
    #[source]
    source: Option<Box<dyn StdError + Send + Sync>>,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub provider_code: Option<String>,
}

impl AdapterError {
    pub fn new(
        kind: AdapterErrorKind,
        provider: ProviderId,
        operation: AdapterOperation,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            provider,
            operation,
            message: message.into(),
            source: None,
            status_code: None,
            request_id: None,
            provider_code: None,
        }
    }

    pub fn with_source<E>(
        kind: AdapterErrorKind,
        provider: ProviderId,
        operation: AdapterOperation,
        message: impl Into<String>,
        source: E,
    ) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            kind,
            provider,
            operation,
            message: message.into(),
            source: Some(Box::new(source)),
            status_code: None,
            request_id: None,
            provider_code: None,
        }
    }

    pub fn source_ref(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}
