use crate::error::{AdapterErrorKind, AdapterOperation};
use crate::openai_spec::OpenAiSpecError;
use agent_core::types::ProviderId;

use super::translator::{OpenAiTranslator, OpenAiTranslatorError};

#[test]
fn maps_openai_encode_error_into_adapter_error() {
    let translator_error =
        OpenAiTranslatorError::Encode(OpenAiSpecError::validation("bad request"));
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenAi);
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
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert_eq!(adapter_error.message, "bad response");
}

#[test]
fn maps_openai_encode_kind_error_into_adapter_error() {
    let translator_error = OpenAiTranslatorError::Encode(OpenAiSpecError::Encode {
        message: "encode failed".to_string(),
        source: Some(Box::new(std::io::Error::other("invalid json"))),
    });
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::EncodeRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Encode);
    assert_eq!(adapter_error.message, "encode failed");
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn maps_openai_error_preserves_source_chain() {
    let translator_error = OpenAiTranslatorError::Encode(OpenAiSpecError::Encode {
        message: "encode failed".to_string(),
        source: Some(Box::new(std::io::Error::other("invalid json"))),
    });
    let adapter_error: crate::error::AdapterError = translator_error.into();

    let translator_source = adapter_error
        .source_ref()
        .expect("adapter error should preserve translator source");
    assert!(
        translator_source
            .to_string()
            .contains("OpenAI encode error"),
        "expected translator error context, got: {translator_source}"
    );

    let spec_source = translator_source
        .source()
        .expect("translator source should expose spec source");
    assert!(
        spec_source.to_string().contains("encode error"),
        "expected spec error context, got: {spec_source}"
    );

    let leaf_source = spec_source
        .source()
        .expect("spec source should expose leaf source");
    assert!(
        leaf_source.to_string().contains("invalid json"),
        "expected leaf source context, got: {leaf_source}"
    );
}

#[test]
fn maps_openai_upstream_error_into_adapter_error() {
    let translator_error = OpenAiTranslatorError::Decode(OpenAiSpecError::Upstream {
        message: "provider said no".to_string(),
    });
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert_eq!(adapter_error.message, "provider said no");
}

#[test]
fn maps_openai_protocol_violation_error_into_adapter_error() {
    let translator_error = OpenAiTranslatorError::Decode(OpenAiSpecError::protocol_violation(
        "response shape mismatch",
    ));
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::ProtocolViolation);
    assert_eq!(adapter_error.message, "response shape mismatch");
}

#[test]
fn maps_openai_unsupported_feature_error_into_adapter_error() {
    let translator_error =
        OpenAiTranslatorError::Decode(OpenAiSpecError::unsupported_feature("json_schema"));
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenAi);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::UnsupportedFeature);
    assert_eq!(adapter_error.message, "json_schema");
}

#[test]
fn openai_translator_is_constructible() {
    let _ = OpenAiTranslator;
}
