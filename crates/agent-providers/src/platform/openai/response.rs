use agent_core::{ProviderKind, Response, ResponseFormat};
use serde_json::Value;

use crate::error::{AdapterError, ProviderErrorInfo};
use crate::openai_family::decode::parse_openai_error_value;

pub(crate) fn decode_response_override(
    _provider: ProviderKind,
    _body: Value,
    _requested_format: &ResponseFormat,
) -> Option<Result<Response, AdapterError>> {
    None
}

pub(crate) fn refine_family_decode_error(body: &Value, error: AdapterError) -> AdapterError {
    refine_openai_compatible_error_metadata(body, error)
}

pub(crate) fn decode_provider_error(body: &Value) -> Option<ProviderErrorInfo> {
    let envelope = parse_openai_error_value(body)?;
    Some(ProviderErrorInfo {
        provider_code: envelope.code.or(envelope.error_type),
        message: None,
        kind: None,
    })
}

pub(crate) fn refine_openai_compatible_error_metadata(
    body: &Value,
    mut error: AdapterError,
) -> AdapterError {
    if let Some(envelope) = parse_openai_error_value(body)
        && let Some(provider_code) = envelope.code.as_deref().or(envelope.error_type.as_deref())
    {
        error = error.with_provider_code(provider_code);
    }

    error
}
