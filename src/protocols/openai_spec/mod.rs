use std::error::Error as StdError;

use serde_json::Value;
use thiserror::Error;

pub(crate) mod decode;
pub(crate) mod encode;
mod schema_rules;

#[cfg(test)]
mod test;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenAiEncodedRequest {
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenAiDecodeEnvelope {
    pub body: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenAiSpecErrorKind {
    Validation,
    Encode,
    Decode,
    ProtocolViolation,
    UnsupportedFeature,
}

#[derive(Debug, Error)]
pub(crate) enum OpenAiSpecError {
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
    #[error("protocol violation: {message}")]
    ProtocolViolation { message: String },
    #[error("unsupported feature: {message}")]
    UnsupportedFeature { message: String },
}

impl OpenAiSpecError {
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    pub(crate) fn encode_with_source<E>(message: impl Into<String>, source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::Encode {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn protocol_violation(message: impl Into<String>) -> Self {
        Self::ProtocolViolation {
            message: message.into(),
        }
    }

    pub(crate) fn unsupported_feature(message: impl Into<String>) -> Self {
        Self::UnsupportedFeature {
            message: message.into(),
        }
    }

    pub(crate) fn kind(&self) -> OpenAiSpecErrorKind {
        match self {
            Self::Validation { .. } => OpenAiSpecErrorKind::Validation,
            Self::Encode { .. } => OpenAiSpecErrorKind::Encode,
            Self::Decode { .. } => OpenAiSpecErrorKind::Decode,
            Self::ProtocolViolation { .. } => OpenAiSpecErrorKind::ProtocolViolation,
            Self::UnsupportedFeature { .. } => OpenAiSpecErrorKind::UnsupportedFeature,
        }
    }

    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Validation { message } => message,
            Self::Encode { message, .. } => message,
            Self::Decode { message, .. } => message,
            Self::ProtocolViolation { message } => message,
            Self::UnsupportedFeature { message } => message,
        }
    }
}
