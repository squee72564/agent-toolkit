use std::collections::BTreeMap;

use serde_json::{Map, json};

use crate::error::{AdapterErrorKind, AdapterOperation, AdapterProtocol};
use crate::openai_spec::{OpenAiDecodeEnvelope, OpenAiSpecError};
use crate::translator_contract::ProtocolTranslator;
use agent_core::types::{ContentPart, Message, MessageRole, Request, ResponseFormat, ToolChoice};

use super::translator::{OpenRouterOverrides, OpenRouterTranslator, OpenRouterTranslatorError};

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
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.protocol, AdapterProtocol::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::EncodeRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert_eq!(adapter_error.message, "bad request");
}

#[test]
fn maps_openrouter_upstream_error_into_adapter_error() {
    let translator_error = OpenRouterTranslatorError::Decode(OpenAiSpecError::Upstream {
        message: "provider failure".to_string(),
    });
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.protocol, AdapterProtocol::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert_eq!(adapter_error.message, "provider failure");
}

#[test]
fn openrouter_translator_reuses_openai_spec_encoder() {
    let translator = OpenRouterTranslator::default();
    let encoded = translator
        .encode_request(&base_request())
        .expect("encoding should succeed");

    assert_eq!(encoded.body["model"], "openai/gpt-4.1-mini");
    assert!(encoded.body["input"].is_array());
}

#[test]
fn openrouter_translator_preserves_openai_encode_warnings() {
    let translator = OpenRouterTranslator::default();
    let mut request = base_request();
    request.top_p = Some(0.9);
    request.stop = vec!["DONE".to_string()];

    let encoded = translator
        .encode_request(&request)
        .expect("encoding should succeed");

    let top_p = encoded.body["top_p"]
        .as_f64()
        .expect("top_p should be numeric");
    assert!((top_p - 0.9).abs() < 1e-6);
    assert_eq!(encoded.body["stop"], json!(["DONE"]));
    assert!(
        encoded
            .warnings
            .iter()
            .all(|w| w.code != "openai.encode.ignored_top_p"),
    );
    assert!(
        encoded
            .warnings
            .iter()
            .all(|w| w.code != "openai.encode.ignored_stop"),
    );
}

#[test]
fn openrouter_translator_applies_typed_overrides() {
    let overrides = OpenRouterOverrides {
        max_tokens: Some(384),
        user: Some("user-1".to_string()),
        route: Some("fallback".to_string()),
        parallel_tool_calls: Some(true),
        ..OpenRouterOverrides::default()
    };
    let translator = OpenRouterTranslator::new(overrides);

    let encoded = translator
        .encode_request(&base_request())
        .expect("encoding should succeed");

    assert_eq!(encoded.body["max_tokens"], 384);
    assert_eq!(encoded.body["user"], "user-1");
    assert_eq!(encoded.body["route"], "fallback");
    assert_eq!(encoded.body["parallel_tool_calls"], true);
}

#[test]
fn openrouter_translator_extra_overrides_take_precedence() {
    let mut extra = Map::new();
    extra.insert("user".to_string(), json!("from-extra"));
    extra.insert("max_tokens".to_string(), json!(777));

    let overrides = OpenRouterOverrides {
        user: Some("from-typed".to_string()),
        max_tokens: Some(111),
        extra,
        ..OpenRouterOverrides::default()
    };
    let translator = OpenRouterTranslator::new(overrides);

    let encoded = translator
        .encode_request(&base_request())
        .expect("encoding should succeed");

    assert_eq!(encoded.body["user"], "from-extra");
    assert_eq!(encoded.body["max_tokens"], 777);
}

#[test]
fn openrouter_decode_uses_openai_path_when_payload_is_openai_compatible() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "openai/gpt-4.1-mini",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "hello from openai format"
                }]
            }],
            "usage": {
                "input_tokens": 3,
                "output_tokens": 4,
                "total_tokens": 7
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = translator
        .decode_request(&payload)
        .expect("decode should succeed");

    assert_eq!(response.model, "openai/gpt-4.1-mini");
    assert_eq!(response.output.content.len(), 1);
    assert!(
        response
            .warnings
            .iter()
            .all(|w| w.code != "openrouter.decode.fallback_chat_completions")
    );
}

#[test]
fn openrouter_decode_falls_back_to_chat_completions_shape() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-4.1-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": "hello from openrouter format"
                }
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 6,
                "total_tokens": 11
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = translator
        .decode_request(&payload)
        .expect("decode should succeed");

    assert_eq!(response.model, "openai/gpt-4.1-mini");
    assert_eq!(
        response.output.content,
        vec![ContentPart::Text {
            text: "hello from openrouter format".to_string()
        }]
    );
    assert_eq!(response.usage.input_tokens, Some(5));
    assert_eq!(response.usage.output_tokens, Some(6));
    assert_eq!(response.usage.total_tokens, Some(11));
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openrouter.decode.fallback_chat_completions")
    );
}

#[test]
fn openrouter_decode_returns_combined_error_when_both_paths_fail() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "choices": [{}]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = translator
        .decode_request(&payload)
        .expect_err("decode should fail");

    match error {
        OpenRouterTranslatorError::Decode(spec_error) => {
            assert!(
                spec_error
                    .message()
                    .contains("openai-compatible decode failed")
            );
            assert!(
                spec_error
                    .message()
                    .contains("openrouter fallback decode failed")
            );
        }
        OpenRouterTranslatorError::Encode(_) => panic!("expected decode error"),
    }
}
