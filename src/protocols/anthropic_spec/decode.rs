use crate::core::types::Response;

use super::{AnthropicDecodeEnvelope, AnthropicSpecError};

pub(crate) fn decode_anthropic_response(
    _payload: &AnthropicDecodeEnvelope,
) -> Result<Response, AnthropicSpecError> {
    Err(AnthropicSpecError::unsupported_feature(
        "Anthropic decode not implemented",
    ))
}
