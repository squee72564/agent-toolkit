use agent_core::{Request, Response, ResponseFormat};
use serde_json::Value;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_spec::{OpenAiDecodeEnvelope, OpenAiSpecError, OpenAiSpecErrorKind};

pub(crate) fn decode_response_json(
    body: Value,
    requested_format: &ResponseFormat,
) -> Result<Response, AdapterError> {
    let _ = std::marker::PhantomData::<Request>;
    crate::openai_spec::decode::decode_openai_response(&OpenAiDecodeEnvelope {
        body,
        requested_response_format: requested_format.clone(),
    })
    .map_err(map_openai_decode_error)
}

fn map_openai_decode_error(error: OpenAiSpecError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::OpenAi,
        AdapterOperation::DecodeResponse,
        message,
        error,
    )
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
