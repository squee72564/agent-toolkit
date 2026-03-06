use crate::anthropic_spec::decode::decode_anthropic_response;
use crate::anthropic_spec::encode::encode_anthropic_request;
use crate::anthropic_spec::{
    AnthropicDecodeEnvelope, AnthropicEncodedRequest, AnthropicSpecError, AnthropicSpecErrorKind,
};
use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::translator_contract::ProtocolTranslator;
use agent_core::types::{ProviderId, Request, Response};
use thiserror::Error;

#[derive(Debug, Default, Clone, Copy)]
pub struct AnthropicTranslator;

#[derive(Debug, Error)]
pub enum AnthropicTranslatorError {
    #[error("Anthropic encode error: {0}")]
    Encode(#[source] AnthropicSpecError),
    #[error("Anthropic decode error: {0}")]
    Decode(#[source] AnthropicSpecError),
}

impl ProtocolTranslator for AnthropicTranslator {
    type RequestPayload = AnthropicEncodedRequest;
    type ResponsePayload = AnthropicDecodeEnvelope;
    type Error = AnthropicTranslatorError;

    fn encode_request(&self, req: Request) -> Result<Self::RequestPayload, Self::Error> {
        encode_anthropic_request(req).map_err(AnthropicTranslatorError::Encode)
    }

    fn decode_request(&self, payload: Self::ResponsePayload) -> Result<Response, Self::Error> {
        decode_anthropic_response(payload).map_err(AnthropicTranslatorError::Decode)
    }
}

impl From<AnthropicTranslatorError> for AdapterError {
    fn from(error: AnthropicTranslatorError) -> Self {
        let (operation, spec_error) = match &error {
            AnthropicTranslatorError::Encode(spec_error) => {
                (AdapterOperation::EncodeRequest, spec_error)
            }
            AnthropicTranslatorError::Decode(spec_error) => {
                (AdapterOperation::DecodeResponse, spec_error)
            }
        };

        AdapterError::with_source(
            map_spec_error_kind(spec_error.kind()),
            ProviderId::Anthropic,
            operation,
            spec_error.message().to_string(),
            error,
        )
    }
}

fn map_spec_error_kind(kind: AnthropicSpecErrorKind) -> AdapterErrorKind {
    match kind {
        AnthropicSpecErrorKind::Validation => AdapterErrorKind::Validation,
        AnthropicSpecErrorKind::Encode => AdapterErrorKind::Encode,
        AnthropicSpecErrorKind::Decode => AdapterErrorKind::Decode,
        AnthropicSpecErrorKind::Upstream => AdapterErrorKind::Upstream,
        AnthropicSpecErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        AnthropicSpecErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}
