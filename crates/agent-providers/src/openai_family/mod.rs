//! OpenAI-family wire-format payload types and spec-level errors.
//!
//! This module defines the protocol-level request/response envelopes used by
//! the OpenAI-compatible family codec. These types sit below the public
//! adapter layer:
//!
//! - request encoders produce [`OpenAiEncodedRequest`]
//! - response decoders consume [`OpenAiDecodeEnvelope`]
//! - wire-format translation failures use [`OpenAiFamilyError`]
//!
//! Most runtime callers should interact with [`crate::adapter`] instead. Reach
//! for this module when working on family-level protocol translation or tests
//! that validate OpenAI-compatible payloads directly.

use std::error::Error as StdError;

use serde_json::Value;
use thiserror::Error;

use agent_core::types::{ResponseFormat, RuntimeWarning};

pub(crate) mod decode;
pub(crate) mod encode;
mod schema_rules;
pub(crate) mod streaming;
pub(crate) mod types;

#[cfg(test)]
mod tests;

/// Encoded OpenAI-family request payload plus non-fatal planning warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiEncodedRequest {
    /// Serialized provider request body.
    pub body: Value,
    /// Non-fatal warnings produced while encoding the request.
    pub warnings: Vec<RuntimeWarning>,
}

/// Input envelope for decoding an OpenAI-family JSON response.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiDecodeEnvelope {
    /// Raw JSON response body.
    pub body: Value,
    /// Response format originally requested by the caller.
    pub requested_response_format: ResponseFormat,
}

/// Category of OpenAI-family translation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiFamilyErrorKind {
    /// Caller input is invalid for the OpenAI-family wire contract.
    Validation,
    /// Encoding a request payload failed.
    Encode,
    /// Decoding a response payload failed.
    Decode,
    /// The provider returned an upstream application error payload.
    Upstream,
    /// The payload violated an expected protocol contract.
    ProtocolViolation,
    /// The requested feature is unsupported by this wire contract.
    UnsupportedFeature,
}

/// OpenAI-family wire-format error used inside provider translations.
#[derive(Debug, Error)]
pub enum OpenAiFamilyError {
    /// Caller input could not be represented by the OpenAI-family contract.
    #[error("validation error: {message}")]
    Validation { message: String },
    /// Building the outbound OpenAI-family payload failed.
    #[error("encode error: {message}")]
    Encode {
        message: String,
        #[source]
        source: Option<Box<dyn StdError + Send + Sync>>,
    },
    /// Parsing or interpreting the inbound OpenAI-family payload failed.
    #[error("decode error: {message}")]
    Decode {
        message: String,
        #[source]
        source: Option<Box<dyn StdError + Send + Sync>>,
    },
    /// The provider reported an application-level error payload.
    #[error("upstream error: {message}")]
    Upstream { message: String },
    /// The payload shape or sequencing violated an expected protocol contract.
    #[error("protocol violation: {message}")]
    ProtocolViolation { message: String },
    #[allow(dead_code)]
    /// The requested behavior is not supported by the OpenAI-family contract.
    #[error("unsupported feature: {message}")]
    UnsupportedFeature { message: String },
}

impl OpenAiFamilyError {
    #[must_use]
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    #[must_use]
    pub(crate) fn encode_with_source<E>(message: impl Into<String>, source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::Encode {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    #[must_use]
    pub(crate) fn protocol_violation(message: impl Into<String>) -> Self {
        Self::ProtocolViolation {
            message: message.into(),
        }
    }

    #[must_use]
    pub(crate) fn decode(message: impl Into<String>) -> Self {
        Self::Decode {
            message: message.into(),
            source: None,
        }
    }

    #[must_use]
    pub(crate) fn upstream(message: impl Into<String>) -> Self {
        Self::Upstream {
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn unsupported_feature(message: impl Into<String>) -> Self {
        Self::UnsupportedFeature {
            message: message.into(),
        }
    }

    #[must_use]
    pub(crate) fn kind(&self) -> OpenAiFamilyErrorKind {
        match self {
            Self::Validation { .. } => OpenAiFamilyErrorKind::Validation,
            Self::Encode { .. } => OpenAiFamilyErrorKind::Encode,
            Self::Decode { .. } => OpenAiFamilyErrorKind::Decode,
            Self::Upstream { .. } => OpenAiFamilyErrorKind::Upstream,
            Self::ProtocolViolation { .. } => OpenAiFamilyErrorKind::ProtocolViolation,
            Self::UnsupportedFeature { .. } => OpenAiFamilyErrorKind::UnsupportedFeature,
        }
    }

    #[must_use]
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Validation { message } => message,
            Self::Encode { message, .. } => message,
            Self::Decode { message, .. } => message,
            Self::Upstream { message } => message,
            Self::ProtocolViolation { message } => message,
            Self::UnsupportedFeature { message } => message,
        }
    }
}
