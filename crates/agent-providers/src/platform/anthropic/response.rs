use agent_core::{Response, ResponseFormat};
use serde_json::Value;

use crate::anthropic_family::{AnthropicDecodeEnvelope, AnthropicSpecError, AnthropicSpecErrorKind};
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

fn map_anthropic_decode_error(error: AnthropicSpecError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::Anthropic,
        AdapterOperation::DecodeResponse,
        message,
        error,
    )
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
