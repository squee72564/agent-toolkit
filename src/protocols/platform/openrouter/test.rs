use std::collections::BTreeMap;

use crate::core::types::{ContentPart, Message, MessageRole, Request, ResponseFormat, ToolChoice};
use crate::protocols::error::{AdapterErrorKind, AdapterOperation, AdapterProtocol};
use crate::protocols::openai_spec::OpenAiSpecError;
use crate::protocols::translator_contract::ProtocolTranslator;

use super::translator::{OpenRouterTranslator, OpenRouterTranslatorError};

fn base_request() -> Request {
    Request {
        model_id: "openai/gpt-4.1-mini".to_string(),
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "hello".to_string(),
            }],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

#[test]
fn maps_openrouter_encode_error_into_adapter_error() {
    let translator_error =
        OpenRouterTranslatorError::Encode(OpenAiSpecError::validation("bad request"));
    let adapter_error: crate::protocols::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.protocol, AdapterProtocol::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::EncodeRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert_eq!(adapter_error.message, "bad request");
}

#[test]
fn openrouter_translator_reuses_openai_spec_encoder() {
    let translator = OpenRouterTranslator;
    let encoded = translator
        .encode_request(&base_request())
        .expect("encoding should succeed");

    assert_eq!(encoded.body["model"], "openai/gpt-4.1-mini");
    assert!(encoded.body["input"].is_array());
}
