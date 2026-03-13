use agent_core::{Response, ResponseFormat};
use serde_json::Value;

use crate::error::{AdapterError, ProviderErrorInfo};
use crate::platform::openai::response::refine_openai_compatible_error_metadata;

pub(crate) fn decode_response_override(
    _body: Value,
    _requested_format: &ResponseFormat,
) -> Option<Result<Response, AdapterError>> {
    None
}

pub(crate) fn refine_family_decode_error(body: &Value, error: AdapterError) -> AdapterError {
    refine_openai_compatible_error_metadata(body, error)
}

pub(crate) fn decode_provider_error(body: &Value) -> Option<ProviderErrorInfo> {
    crate::platform::openai::response::decode_provider_error(body)
}
