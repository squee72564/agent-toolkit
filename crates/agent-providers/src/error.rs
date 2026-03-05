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
    #[must_use]
    pub fn new(
        kind: AdapterErrorKind,
        provider: ProviderId,
        operation: AdapterOperation,
        message: impl Into<String>,
    ) -> Self {
        Self::build(kind, provider, operation, message, None)
    }

    #[must_use]
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
        Self::build(kind, provider, operation, message, Some(Box::new(source)))
    }

    #[must_use]
    pub fn with_status_code(mut self, status_code: u16) -> Self {
        self.status_code = Some(status_code);
        self
    }

    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = normalize_metadata(request_id.into());
        self
    }

    #[must_use]
    pub fn with_provider_code(mut self, provider_code: impl Into<String>) -> Self {
        self.provider_code = normalize_metadata(provider_code.into());
        self
    }

    #[must_use]
    pub fn source_ref(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        self.source.as_deref()
    }

    fn build(
        kind: AdapterErrorKind,
        provider: ProviderId,
        operation: AdapterOperation,
        message: impl Into<String>,
        source: Option<Box<dyn StdError + Send + Sync>>,
    ) -> Self {
        Self {
            kind,
            provider,
            operation,
            message: message.into(),
            source,
            status_code: None,
            request_id: None,
            provider_code: None,
        }
    }
}

fn normalize_metadata(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}
