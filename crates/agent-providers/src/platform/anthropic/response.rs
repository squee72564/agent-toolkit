use agent_core::{Response, ResponseFormat};
use serde_json::Value;

use crate::anthropic_family::{AnthropicDecodeEnvelope, AnthropicFamilyError, AnthropicFamilyErrorKind};
use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};

pub(crate) fn decode_response_json(
    body: Value,
    requested_format: &ResponseFormat,
) -> Result<Response, AdapterError> {
    crate::anthropic_family::decode::decode_anthropic_response(&AnthropicDecodeEnvelope {
        body,
        requested_response_format: requested_format.clone(),
    })
    .map_err(map_anthropic_decode_error)
}

fn map_anthropic_decode_error(error: AnthropicFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::Anthropic,
        AdapterOperation::DecodeResponse,
        message,
        error,
    )
}

fn map_spec_error_kind(kind: AnthropicFamilyErrorKind) -> AdapterErrorKind {
    match kind {
        AnthropicFamilyErrorKind::Validation => AdapterErrorKind::Validation,
        AnthropicFamilyErrorKind::Encode => AdapterErrorKind::Encode,
        AnthropicFamilyErrorKind::Decode => AdapterErrorKind::Decode,
        AnthropicFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
        AnthropicFamilyErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        AnthropicFamilyErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}
