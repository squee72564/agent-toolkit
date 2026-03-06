use std::collections::BTreeMap;

use serde_json::{Map, json};

use crate::error::{AdapterErrorKind, AdapterOperation};
use crate::openai_spec::{OpenAiDecodeEnvelope, OpenAiSpecError};
use crate::translator_contract::ProtocolTranslator;
use agent_core::types::ProviderId;
use agent_core::types::{
    ContentPart, FinishReason, Message, MessageRole, Request, ResponseFormat, ToolChoice,
};

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

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
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

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert_eq!(adapter_error.message, "provider failure");
}

#[test]
fn maps_openrouter_decode_error_into_adapter_error() {
    let translator_error = OpenRouterTranslatorError::Decode(OpenAiSpecError::Decode {
        message: "bad response".to_string(),
        source: None,
    });
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert_eq!(adapter_error.message, "bad response");
}

#[test]
fn maps_openrouter_encode_kind_error_into_adapter_error() {
    let translator_error = OpenRouterTranslatorError::Encode(OpenAiSpecError::Encode {
        message: "encode failed".to_string(),
        source: Some(Box::new(std::io::Error::other("invalid json"))),
    });
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::EncodeRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Encode);
    assert_eq!(adapter_error.message, "encode failed");
    assert!(adapter_error.source_ref().is_some());
}

#[test]
fn maps_openrouter_protocol_violation_error_into_adapter_error() {
    let translator_error = OpenRouterTranslatorError::Decode(OpenAiSpecError::protocol_violation(
        "response shape mismatch",
    ));
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::ProtocolViolation);
    assert_eq!(adapter_error.message, "response shape mismatch");
}

#[test]
fn maps_openrouter_unsupported_feature_error_into_adapter_error() {
    let translator_error =
        OpenRouterTranslatorError::Decode(OpenAiSpecError::unsupported_feature("json_schema"));
    let adapter_error: crate::error::AdapterError = translator_error.into();

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::UnsupportedFeature);
    assert_eq!(adapter_error.message, "json_schema");
}

#[test]
fn maps_openrouter_error_preserves_source_chain() {
    let translator_error = OpenRouterTranslatorError::Encode(OpenAiSpecError::Encode {
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
            .contains("OpenRouter encode error"),
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
fn openrouter_translator_reuses_openai_spec_encoder() {
    let translator = OpenRouterTranslator::default();
    let encoded = translator
        .encode_request(base_request())
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
        .encode_request(request.clone())
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
fn openrouter_translator_reintroduces_top_p_and_stop_with_fallback_models() {
    let overrides = OpenRouterOverrides {
        fallback_models: vec!["openai/gpt-4.1".to_string()],
        ..OpenRouterOverrides::default()
    };
    let translator = OpenRouterTranslator::new(overrides);
    let mut request = base_request();
    request.top_p = Some(0.9);
    request.stop = vec!["DONE".to_string()];

    let encoded = translator
        .encode_request(request)
        .expect("encoding should succeed");

    assert_eq!(
        encoded.body["models"],
        json!(["openai/gpt-4.1-mini", "openai/gpt-4.1"])
    );
    let top_p = encoded.body["top_p"]
        .as_f64()
        .expect("top_p should be numeric");
    assert!((top_p - 0.9).abs() < 1e-6);
    assert_eq!(encoded.body["stop"], json!(["DONE"]));
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
        .encode_request(base_request())
        .expect("encoding should succeed");

    assert_eq!(encoded.body["max_tokens"], 384);
    assert_eq!(encoded.body["user"], "user-1");
    assert_eq!(encoded.body["route"], "fallback");
    assert_eq!(encoded.body["parallel_tool_calls"], true);
}

#[test]
fn openrouter_translator_rejects_non_finite_frequency_penalty_override() {
    let overrides = OpenRouterOverrides {
        frequency_penalty: Some(f32::NAN),
        ..OpenRouterOverrides::default()
    };
    let translator = OpenRouterTranslator::new(overrides);

    let error = translator
        .encode_request(base_request())
        .expect_err("encoding should fail for non-finite frequency_penalty");

    match error {
        OpenRouterTranslatorError::Encode(spec_error) => {
            assert!(spec_error.message().contains("frequency_penalty"));
            assert!(spec_error.message().contains("must be finite"));
        }
        OpenRouterTranslatorError::Decode(_) => panic!("expected encode error"),
    }
}

#[test]
fn openrouter_translator_rejects_non_finite_presence_penalty_override() {
    let overrides = OpenRouterOverrides {
        presence_penalty: Some(f32::INFINITY),
        ..OpenRouterOverrides::default()
    };
    let translator = OpenRouterTranslator::new(overrides);

    let error = translator
        .encode_request(base_request())
        .expect_err("encoding should fail for non-finite presence_penalty");

    match error {
        OpenRouterTranslatorError::Encode(spec_error) => {
            assert!(spec_error.message().contains("presence_penalty"));
            assert!(spec_error.message().contains("must be finite"));
        }
        OpenRouterTranslatorError::Decode(_) => panic!("expected encode error"),
    }
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
        .encode_request(base_request())
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
        .decode_request(payload.clone())
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
        .decode_request(payload.clone())
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
        .decode_request(payload.clone())
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

#[test]
fn openrouter_decode_does_not_fallback_on_upstream_error() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "error": {
                "message": "upstream hard failure",
                "code": 401
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = translator
        .decode_request(payload.clone())
        .expect_err("decode should fail");

    match error {
        OpenRouterTranslatorError::Decode(spec_error) => {
            assert!(spec_error.message().contains("upstream hard failure"));
            assert!(
                !spec_error
                    .message()
                    .contains("openrouter fallback decode failed")
            );
            assert!(
                !spec_error
                    .message()
                    .contains("openai-compatible decode failed")
            );
        }
        OpenRouterTranslatorError::Encode(_) => panic!("expected decode error"),
    }
}

#[test]
fn openrouter_decode_tool_call_missing_id_generates_warning_and_synthetic_id() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-4.1-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "type": "function",
                        "function": {
                            "name": "lookup_weather",
                            "arguments": "{\"city\":\"SF\"}"
                        }
                    }]
                }
            }]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = translator
        .decode_request(payload.clone())
        .expect("decode should succeed");

    assert_eq!(response.finish_reason, FinishReason::ToolCalls);
    assert!(response.warnings.iter().any(|w| {
        w.code == "openrouter.decode.missing_tool_call_id"
            && w.message.contains("generated synthetic id")
    }));
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openrouter.decode.fallback_chat_completions")
    );
    assert!(response.output.content.iter().any(|part| {
        matches!(
            part,
            ContentPart::ToolCall { tool_call } if tool_call.id == "openrouter_tool_call_0"
                && tool_call.name == "lookup_weather"
                && tool_call.arguments_json == json!({"city":"SF"})
        )
    }));
}

#[test]
fn openrouter_decode_tool_call_missing_name_is_ignored_with_warning() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-4.1-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "arguments": "{\"city\":\"SF\"}"
                        }
                    }]
                }
            }]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = translator
        .decode_request(payload.clone())
        .expect("decode should succeed");

    assert!(
        !response
            .output
            .content
            .iter()
            .any(|part| matches!(part, ContentPart::ToolCall { .. }))
    );
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openrouter.decode.missing_tool_call_name")
    );
}

#[test]
fn openrouter_decode_tool_call_whitespace_name_is_ignored_with_warning() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-4.1-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "   ",
                            "arguments": "{\"city\":\"SF\"}"
                        }
                    }]
                }
            }]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = translator
        .decode_request(payload.clone())
        .expect("decode should succeed");

    assert!(
        !response
            .output
            .content
            .iter()
            .any(|part| matches!(part, ContentPart::ToolCall { .. }))
    );
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openrouter.decode.missing_tool_call_name")
    );
}

#[test]
fn openrouter_decode_invalid_tool_call_arguments_preserve_raw_string_with_warning() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-4.1-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "lookup_weather",
                            "arguments": "{not json"
                        }
                    }]
                }
            }]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = translator
        .decode_request(payload.clone())
        .expect("decode should succeed");

    assert!(response.warnings.iter().any(|w| {
        w.code == "openrouter.decode.invalid_tool_call_arguments"
            && w.message.contains("preserved raw string")
    }));
    assert!(response.output.content.iter().any(|part| {
        matches!(
            part,
            ContentPart::ToolCall { tool_call } if tool_call.arguments_json == json!("{not json")
        )
    }));
}

#[test]
fn openrouter_decode_unknown_finish_reason_emits_warning_and_maps_to_other() {
    let translator = OpenRouterTranslator::default();
    let payload = OpenAiDecodeEnvelope {
        body: json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-4.1-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "provider_custom_reason",
                "message": {
                    "role": "assistant",
                    "content": "hello"
                }
            }]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = translator
        .decode_request(payload.clone())
        .expect("decode should succeed");

    assert_eq!(response.finish_reason, FinishReason::Other);
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openrouter.decode.unknown_finish_reason")
    );
}

#[test]
fn openrouter_decode_structured_output_parse_failure_emits_warning() {
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
                    "content": "not-json"
                }
            }]
        }),
        requested_response_format: ResponseFormat::JsonObject,
    };

    let response = translator
        .decode_request(payload.clone())
        .expect("decode should succeed");

    assert!(response.output.structured_output.is_none());
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openrouter.decode.structured_output_parse_failed")
    );
}
