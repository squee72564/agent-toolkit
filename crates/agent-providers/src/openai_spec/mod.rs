//! OpenAI-family wire-format payload types and spec-level errors.

use std::error::Error as StdError;

use serde_json::Value;
use thiserror::Error;

use agent_core::types::{ResponseFormat, RuntimeWarning};

pub(crate) mod decode;
pub(crate) mod encode;
mod schema_rules;

#[cfg(test)]
mod test;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiSpecErrorKind {
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
pub enum OpenAiSpecError {
    #[error("validation error: {message}")]
    Validation { message: String },
    #[error("encode error: {message}")]
    Encode {
        message: String,
        #[source]
        source: Option<Box<dyn StdError + Send + Sync>>,
    },
    #[error("decode error: {message}")]
    Decode {
        message: String,
        #[source]
        source: Option<Box<dyn StdError + Send + Sync>>,
    },
    #[error("upstream error: {message}")]
    Upstream { message: String },
    #[error("protocol violation: {message}")]
    ProtocolViolation { message: String },
    #[allow(dead_code)]
    #[error("unsupported feature: {message}")]
    UnsupportedFeature { message: String },
}

impl OpenAiSpecError {
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
    pub(crate) fn kind(&self) -> OpenAiSpecErrorKind {
        match self {
            Self::Validation { .. } => OpenAiSpecErrorKind::Validation,
            Self::Encode { .. } => OpenAiSpecErrorKind::Encode,
            Self::Decode { .. } => OpenAiSpecErrorKind::Decode,
            Self::Upstream { .. } => OpenAiSpecErrorKind::Upstream,
            Self::ProtocolViolation { .. } => OpenAiSpecErrorKind::ProtocolViolation,
            Self::UnsupportedFeature { .. } => OpenAiSpecErrorKind::UnsupportedFeature,
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
