use agent_core::{Request, Response, ResponseFormat};
use serde_json::Value;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_family::{OpenAiDecodeEnvelope, OpenAiFamilyError, OpenAiFamilyErrorKind};

pub(crate) fn decode_response_json(
    body: Value,
    requested_format: &ResponseFormat,
) -> Result<Response, AdapterError> {
    let _ = std::marker::PhantomData::<Request>;
    crate::openai_family::decode::decode_openai_response(&OpenAiDecodeEnvelope {
        body,
        requested_response_format: requested_format.clone(),
    })
    .map_err(map_openai_decode_error)
}

fn map_openai_decode_error(error: OpenAiFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::OpenAi,
        AdapterOperation::DecodeResponse,
        message,
        error,
    )
}

fn map_spec_error_kind(kind: OpenAiFamilyErrorKind) -> AdapterErrorKind {
    match kind {
        OpenAiFamilyErrorKind::Validation => AdapterErrorKind::Validation,
        OpenAiFamilyErrorKind::Encode => AdapterErrorKind::Encode,
        OpenAiFamilyErrorKind::Decode => AdapterErrorKind::Decode,
        OpenAiFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
        OpenAiFamilyErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        OpenAiFamilyErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}
