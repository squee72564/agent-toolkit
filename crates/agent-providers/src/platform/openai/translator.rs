use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_spec::decode::decode_openai_response;
use crate::openai_spec::encode::encode_openai_request;
use crate::openai_spec::{
    OpenAiDecodeEnvelope, OpenAiEncodedRequest, OpenAiSpecError, OpenAiSpecErrorKind,
};
use crate::translator_contract::ProtocolTranslator;
use agent_core::types::{ProviderId, Request, Response};
use thiserror::Error;

#[derive(Debug, Default, Clone, Copy)]
pub struct OpenAiTranslator;

#[derive(Debug, Error)]
pub enum OpenAiTranslatorError {
    #[error("OpenAI encode error: {0}")]
    Encode(#[source] OpenAiSpecError),
    #[error("OpenAI decode error: {0}")]
    Decode(#[source] OpenAiSpecError),
}

impl ProtocolTranslator for OpenAiTranslator {
    type RequestPayload = OpenAiEncodedRequest;
    type ResponsePayload = OpenAiDecodeEnvelope;
    type Error = OpenAiTranslatorError;

    fn encode_request(&self, req: &Request) -> Result<Self::RequestPayload, Self::Error> {
        encode_openai_request(req).map_err(OpenAiTranslatorError::Encode)
    }

    fn decode_request(&self, payload: &Self::ResponsePayload) -> Result<Response, Self::Error> {
        decode_openai_response(payload).map_err(OpenAiTranslatorError::Decode)
    }
}

impl From<OpenAiTranslatorError> for AdapterError {
    fn from(error: OpenAiTranslatorError) -> Self {
        let (operation, spec_error) = match &error {
            OpenAiTranslatorError::Encode(spec_error) => {
                (AdapterOperation::EncodeRequest, spec_error)
            }
            OpenAiTranslatorError::Decode(spec_error) => {
                (AdapterOperation::DecodeResponse, spec_error)
            }
        };

        AdapterError::with_source(
            map_spec_error_kind(spec_error.kind()),
            ProviderId::OpenAi,
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
