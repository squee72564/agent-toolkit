use crate::protocols::error::{AdapterErrorKind, AdapterOperation, AdapterProtocol};
use crate::protocols::openai_spec::OpenAiSpecError;

use super::translator::{OpenAiTranslator, OpenAiTranslatorError};

#[test]
fn maps_openai_encode_error_into_adapter_error() {
    let translator_error =
        OpenAiTranslatorError::Encode(OpenAiSpecError::validation("bad request"));
    let adapter_error: crate::protocols::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.protocol, AdapterProtocol::OpenAI);
    assert_eq!(adapter_error.operation, AdapterOperation::EncodeRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert_eq!(adapter_error.message, "bad request");
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn maps_openai_decode_error_into_adapter_error() {
    let translator_error = OpenAiTranslatorError::Decode(OpenAiSpecError::Decode {
        message: "bad response".to_string(),
        source: None,
    });
    let adapter_error: crate::protocols::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.protocol, AdapterProtocol::OpenAI);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert_eq!(adapter_error.message, "bad response");
}

#[test]
fn openai_translator_is_constructible() {
    let _ = OpenAiTranslator;
}
