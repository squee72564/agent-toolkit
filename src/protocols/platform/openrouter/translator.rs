use serde_json::Value;

use crate::core::types::{Request, Response};
use crate::protocols::error::{AdapterError, AdapterErrorKind, AdapterOperation, AdapterProtocol};
use crate::protocols::openai_spec::decode::decode_openai_response;
use crate::protocols::openai_spec::encode::encode_openai_request;
use crate::protocols::openai_spec::{
    OpenAiDecodeEnvelope, OpenAiEncodedRequest, OpenAiSpecError, OpenAiSpecErrorKind,
};
use crate::protocols::translator_contract::ProtocolTranslator;
use thiserror::Error;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OpenRouterTranslator;

#[derive(Debug, Error)]
pub(crate) enum OpenRouterTranslatorError {
    #[error("OpenRouter encode error: {0}")]
    Encode(#[source] OpenAiSpecError),
    #[error("OpenRouter decode error: {0}")]
    Decode(#[source] OpenAiSpecError),
}

impl ProtocolTranslator for OpenRouterTranslator {
    type RequestPayload = OpenAiEncodedRequest;
    type ResponsePayload = OpenAiDecodeEnvelope;
    type Error = OpenRouterTranslatorError;

    fn encode_request(&self, req: &Request) -> Result<Self::RequestPayload, Self::Error> {
        let mut encoded = encode_openai_request(req).map_err(OpenRouterTranslatorError::Encode)?;
        apply_openrouter_overrides(&mut encoded.body);
        Ok(encoded)
    }

    fn decode_request(&self, payload: &Self::ResponsePayload) -> Result<Response, Self::Error> {
        decode_openai_response(payload).map_err(OpenRouterTranslatorError::Decode)
    }
}

impl From<OpenRouterTranslatorError> for AdapterError {
    fn from(error: OpenRouterTranslatorError) -> Self {
        let (operation, spec_error) = match &error {
            OpenRouterTranslatorError::Encode(spec_error) => {
                (AdapterOperation::EncodeRequest, spec_error)
            }
            OpenRouterTranslatorError::Decode(spec_error) => {
                (AdapterOperation::DecodeResponse, spec_error)
            }
        };

        AdapterError::with_source(
            map_spec_error_kind(spec_error.kind()),
            AdapterProtocol::OpenRouter,
            operation,
            spec_error.message().to_string(),
            error,
        )
    }
}

fn map_spec_error_kind(kind: OpenAiSpecErrorKind) -> AdapterErrorKind {
    match kind {
        OpenAiSpecErrorKind::Validation => AdapterErrorKind::Validation,
        OpenAiSpecErrorKind::Encode => AdapterErrorKind::Encode,
        OpenAiSpecErrorKind::Decode => AdapterErrorKind::Decode,
        OpenAiSpecErrorKind::Upstream => AdapterErrorKind::Upstream,
        OpenAiSpecErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        OpenAiSpecErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}

fn apply_openrouter_overrides(_request_body: &mut Value) {}
