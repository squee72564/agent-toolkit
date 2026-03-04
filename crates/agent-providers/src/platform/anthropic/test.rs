use crate::anthropic_spec::AnthropicSpecError;
use crate::error::{AdapterErrorKind, AdapterOperation, AdapterProtocol};

use super::translator::{AnthropicTranslator, AnthropicTranslatorError};

#[test]
fn maps_anthropic_encode_error_into_adapter_error() {
    let translator_error =
        AnthropicTranslatorError::Encode(AnthropicSpecError::validation("bad request"));
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.protocol, AdapterProtocol::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::EncodeRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert_eq!(adapter_error.message, "bad request");
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn maps_anthropic_decode_error_into_adapter_error() {
    let translator_error = AnthropicTranslatorError::Decode(AnthropicSpecError::Decode {
        message: "bad response".to_string(),
        source: None,
    });
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.protocol, AdapterProtocol::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert_eq!(adapter_error.message, "bad response");
}

#[test]
fn maps_anthropic_upstream_error_into_adapter_error() {
    let translator_error = AnthropicTranslatorError::Decode(AnthropicSpecError::Upstream {
        message: "provider said no".to_string(),
    });
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.protocol, AdapterProtocol::Anthropic);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert_eq!(adapter_error.message, "provider said no");
}

#[test]
fn anthropic_translator_is_constructible() {
    let _ = AnthropicTranslator;
}
