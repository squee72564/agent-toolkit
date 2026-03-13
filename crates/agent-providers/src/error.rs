//! Normalized adapter-layer error types.

use std::error::Error as StdError;

use agent_core::types::ProviderId;
use thiserror::Error;

/// Adapter pipeline stage that produced an [`AdapterError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterOperation {
    /// Failed while translating a canonical request into a provider request.
    PlanRequest,
    /// Failed while decoding a provider response payload.
    DecodeResponse,
    /// Failed while building an HTTP request or platform configuration.
    BuildHttpRequest,
    /// Failed while converting a raw provider stream event into canonical
    /// stream events.
    ProjectStreamEvent,
    /// Failed while finalizing a provider stream after input exhaustion.
    FinalizeStream,
}

/// Category of failure surfaced by a provider adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterErrorKind {
    /// Caller input is invalid for the provider contract.
    Validation,
    /// Provider request encoding failed.
    Encode,
    /// Provider response decoding failed.
    Decode,
    /// Provider payload order or shape violated an expected protocol contract.
    ProtocolViolation,
    /// The requested feature is not supported by the provider contract.
    UnsupportedFeature,
    /// The provider returned an application-level upstream error.
    Upstream,
    /// Transport integration failed before or around provider processing.
    Transport,
}

/// Typed provider-layer error details extracted before runtime normalization.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderErrorInfo {
    /// Provider-specific error code, when available.
    pub provider_code: Option<String>,
    /// Human-readable provider error message, when available.
    pub message: Option<String>,
    /// Adapter-layer error kind inferred from the provider error payload.
    pub kind: Option<AdapterErrorKind>,
}

impl ProviderErrorInfo {
    /// Merges provider-overlay fields over family-decoded fields.
    #[must_use]
    pub fn refined_with(mut self, overlay: Self) -> Self {
        if overlay.provider_code.is_some() {
            self.provider_code = overlay.provider_code;
        }
        if overlay.message.is_some() {
            self.message = overlay.message;
        }
        if overlay.kind.is_some() {
            self.kind = overlay.kind;
        }
        self
    }
}

/// Normalized adapter-layer error used by the runtime.
#[derive(Debug, Error)]
#[error("{provider:?}::{operation:?}::{kind:?}: {message}")]
pub struct AdapterError {
    /// High-level adapter error category.
    pub kind: AdapterErrorKind,
    /// Provider that produced the error.
    pub provider: ProviderId,
    /// Adapter pipeline stage that failed.
    pub operation: AdapterOperation,
    /// Human-readable error message.
    pub message: String,
    #[source]
    source: Option<Box<dyn StdError + Send + Sync>>,
    /// HTTP status code returned by the provider, when available.
    pub status_code: Option<u16>,
    /// Provider request identifier, when available.
    pub request_id: Option<String>,
    /// Provider-specific error code, when available.
    pub provider_code: Option<String>,
}

impl AdapterError {
    /// Creates an adapter error without a source error.
    #[must_use]
    pub fn new(
        kind: AdapterErrorKind,
        provider: ProviderId,
        operation: AdapterOperation,
        message: impl Into<String>,
    ) -> Self {
        Self::build(kind, provider, operation, message, None)
    }

    /// Creates an adapter error and preserves an underlying source error.
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

    /// Attaches an HTTP status code to the error.
    #[must_use]
    pub fn with_status_code(mut self, status_code: u16) -> Self {
        self.status_code = Some(status_code);
        self
    }

    /// Attaches a provider request identifier to the error.
    ///
    /// Blank values are normalized to `None`.
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = normalize_metadata(request_id.into());
        self
    }

    /// Attaches a provider-specific error code to the error.
    ///
    /// Blank values are normalized to `None`.
    #[must_use]
    pub fn with_provider_code(mut self, provider_code: impl Into<String>) -> Self {
        self.provider_code = normalize_metadata(provider_code.into());
        self
    }

    /// Returns the preserved source error, if one exists.
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
