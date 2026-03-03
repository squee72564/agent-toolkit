use crate::core::types::Request;

use super::{AnthropicEncodedRequest, AnthropicSpecError};

pub(crate) fn encode_anthropic_request(
    _req: &Request,
) -> Result<AnthropicEncodedRequest, AnthropicSpecError> {
    Err(AnthropicSpecError::unsupported_feature(
        "Anthropic encode not implemented",
    ))
}
