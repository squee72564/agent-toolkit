use crate::core::types::Response;

use super::{OpenAiDecodeEnvelope, OpenAiSpecError};

pub(crate) fn decode_openai_response(
    payload: &OpenAiDecodeEnvelope,
) -> Result<Response, OpenAiSpecError> {
    if !payload.body.is_object() {
        return Err(OpenAiSpecError::Decode {
            message: "OpenAI decode envelope body must be a JSON object".to_string(),
            source: None,
        });
    }

    Err(OpenAiSpecError::unsupported_feature(
        "OpenAI response decoding is not implemented yet",
    ))
}
