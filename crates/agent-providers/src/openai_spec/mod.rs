use std::error::Error as StdError;

use serde_json::Value;
use thiserror::Error;

use agent_core::types::{ResponseFormat, RuntimeWarning};

pub(crate) mod decode;
pub(crate) mod encode;
mod schema_rules;

#[cfg(test)]
mod test;

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiEncodedRequest {
    pub body: Value,
    pub warnings: Vec<RuntimeWarning>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiDecodeEnvelope {
    pub body: Value,
    pub requested_response_format: ResponseFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAiErrorEnvelope {
    pub message: String,
    pub code: Option<String>,
    pub error_type: Option<String>,
    pub param: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiSpecErrorKind {
    Validation,
    Encode,
    Decode,
    Upstream,
    ProtocolViolation,
    UnsupportedFeature,
}

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

    pub(crate) fn decode(message: impl Into<String>) -> Self {
        Self::Decode {
            message: message.into(),
            source: None,
        }
    }

    pub(crate) fn upstream(message: impl Into<String>) -> Self {
        Self::Upstream {
            message: message.into(),
        }
    }

    #[allow(dead_code)]
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
            Self::Upstream { .. } => OpenAiSpecErrorKind::Upstream,
            Self::ProtocolViolation { .. } => OpenAiSpecErrorKind::ProtocolViolation,
            Self::UnsupportedFeature { .. } => OpenAiSpecErrorKind::UnsupportedFeature,
        }
    }

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
