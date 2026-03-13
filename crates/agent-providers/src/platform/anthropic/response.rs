use agent_core::{Response, ResponseFormat};
use serde_json::Value;

use crate::anthropic_family::decode::parse_anthropic_error_value;
use crate::error::{AdapterError, ProviderErrorInfo};

pub(crate) fn decode_response_override(
    _body: Value,
    _requested_format: &ResponseFormat,
) -> Option<Result<Response, AdapterError>> {
    None
}

pub(crate) fn refine_family_decode_error(body: &Value, mut error: AdapterError) -> AdapterError {
    let Some(root) = body.as_object() else {
        return error;
    };

    if let Some(envelope) = parse_anthropic_error_value(root) {
        if let Some(provider_code) = envelope.error_type.as_deref() {
            error = error.with_provider_code(provider_code);
        }
        if let Some(request_id) = envelope.request_id.as_deref() {
            error = error.with_request_id(request_id);
        }
    }

    error
}

pub(crate) fn decode_provider_error(body: &Value) -> Option<ProviderErrorInfo> {
    let root = body.as_object()?;
    let envelope = parse_anthropic_error_value(root)?;
    Some(ProviderErrorInfo {
        provider_code: envelope.error_type,
        message: None,
        kind: None,
    })
}
