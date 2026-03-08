use std::collections::BTreeMap;

use serde_json::{Map, json};

use crate::error::{AdapterErrorKind, AdapterOperation};
use agent_core::types::ProviderId;
use agent_core::types::{
    ContentPart, FinishReason, Message, MessageRole, Request, ResponseFormat, ToolChoice,
};

use super::{request, response};
use crate::platform::openrouter::request::OpenRouterOverrides;

fn base_request() -> Request {
    Request {
        model_id: "openai/gpt-4.1-mini".to_string(),
        stream: false,
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
fn openrouter_request_error_maps_into_adapter_error() {
    let adapter_error = request::plan_request(
        Request {
            model_id: String::new(),
            ..base_request()
        },
        &OpenRouterOverrides::default(),
    )
    .expect_err("planning should fail");

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::PlanRequest);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Validation);
    assert!(adapter_error.message.contains("model_id must not be empty"));
}

#[test]
fn openrouter_upstream_error_maps_into_adapter_error() {
    let adapter_error = response::decode_response_json(
        json!({"error":{"message":"provider failure","code":401}}),
        &ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Upstream);
    assert!(adapter_error.message.contains("provider failure"));
}

#[test]
fn openrouter_decode_error_maps_into_adapter_error() {
    let adapter_error =
        response::decode_response_json(json!("bad response"), &ResponseFormat::Text)
            .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert!(
        adapter_error
            .message
            .contains("openai-compatible decode failed")
    );
}

#[test]
fn openrouter_protocol_violation_error_maps_into_adapter_error() {
    let adapter_error = response::decode_response_json(
        json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-4.1-mini",
            "choices": "bad"
        }),
        &ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert_eq!(adapter_error.provider, ProviderId::OpenRouter);
    assert_eq!(adapter_error.operation, AdapterOperation::DecodeResponse);
    assert_eq!(adapter_error.kind, AdapterErrorKind::Decode);
    assert!(!adapter_error.message.is_empty());
}

#[test]
fn openrouter_request_error_preserves_source_chain() {
    let adapter_error = request::plan_request(
        Request {
            model_id: String::new(),
            ..base_request()
        },
        &OpenRouterOverrides::default(),
    )
    .expect_err("planning should fail");

    let spec_source = adapter_error
        .source_ref()
        .expect("adapter error should preserve spec source");
    assert!(
        spec_source.to_string().contains("validation error"),
        "expected spec error context, got: {spec_source}"
    );
}

#[test]
fn openrouter_request_reuses_openai_spec_encoder() {
    let encoded = request::plan_request(base_request(), &OpenRouterOverrides::default())
        .expect("planning should succeed");

    assert_eq!(encoded.body["model"], "openai/gpt-4.1-mini");
    assert!(encoded.body["input"].is_array());
}

#[test]
fn openrouter_request_preserves_openai_encode_warnings() {
    let mut request = base_request();
    request.top_p = Some(0.9);
    request.stop = vec!["DONE".to_string()];

    let encoded = request::plan_request(request.clone(), &OpenRouterOverrides::default())
        .expect("planning should succeed");

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
fn openrouter_request_reintroduces_top_p_and_stop_with_fallback_models() {
    let overrides = OpenRouterOverrides {
        fallback_models: vec!["openai/gpt-4.1".to_string()],
        ..OpenRouterOverrides::default()
    };
    let mut request = base_request();
    request.top_p = Some(0.9);
    request.stop = vec!["DONE".to_string()];

    let encoded = request::plan_request(request, &overrides).expect("planning should succeed");

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
fn openrouter_request_applies_typed_overrides() {
    let overrides = OpenRouterOverrides {
        max_tokens: Some(384),
        user: Some("user-1".to_string()),
        route: Some("fallback".to_string()),
        parallel_tool_calls: Some(true),
        ..OpenRouterOverrides::default()
    };
    let encoded =
        request::plan_request(base_request(), &overrides).expect("planning should succeed");

    assert_eq!(encoded.body["max_tokens"], 384);
    assert_eq!(encoded.body["user"], "user-1");
    assert_eq!(encoded.body["route"], "fallback");
    assert_eq!(encoded.body["parallel_tool_calls"], true);
}

#[test]
fn openrouter_request_rejects_non_finite_frequency_penalty_override() {
    let overrides = OpenRouterOverrides {
        frequency_penalty: Some(f32::NAN),
        ..OpenRouterOverrides::default()
    };
    let error = request::plan_request(base_request(), &overrides)
        .expect_err("planning should fail for non-finite frequency_penalty");
    assert!(error.message.contains("frequency_penalty"));
    assert!(error.message.contains("must be finite"));
}

#[test]
fn openrouter_request_rejects_non_finite_presence_penalty_override() {
    let overrides = OpenRouterOverrides {
        presence_penalty: Some(f32::INFINITY),
        ..OpenRouterOverrides::default()
    };
    let error = request::plan_request(base_request(), &overrides)
        .expect_err("planning should fail for non-finite presence_penalty");
    assert!(error.message.contains("presence_penalty"));
    assert!(error.message.contains("must be finite"));
}

#[test]
fn openrouter_request_extra_overrides_take_precedence() {
    let mut extra = Map::new();
    extra.insert("user".to_string(), json!("from-extra"));
    extra.insert("max_tokens".to_string(), json!(777));

    let overrides = OpenRouterOverrides {
        user: Some("from-typed".to_string()),
        max_tokens: Some(111),
        extra,
        ..OpenRouterOverrides::default()
    };
    let encoded =
        request::plan_request(base_request(), &overrides).expect("planning should succeed");

    assert_eq!(encoded.body["user"], "from-extra");
    assert_eq!(encoded.body["max_tokens"], 777);
}

#[test]
fn openrouter_decode_uses_openai_path_when_payload_is_openai_compatible() {
    let response = response::decode_response_json(
        json!({
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
        &ResponseFormat::Text,
    )
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
    let response = response::decode_response_json(
        json!({
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
        &ResponseFormat::Text,
    )
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
    let error = response::decode_response_json(
        json!({
            "choices": [{}]
        }),
        &ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert!(error.message.contains("openai-compatible decode failed"));
    assert!(error.message.contains("openrouter fallback decode failed"));
}

#[test]
fn openrouter_decode_does_not_fallback_on_upstream_error() {
    let error = response::decode_response_json(
        json!({
            "error": {
                "message": "upstream hard failure",
                "code": 401
            }
        }),
        &ResponseFormat::Text,
    )
    .expect_err("decode should fail");

    assert!(error.message.contains("upstream hard failure"));
    assert!(!error.message.contains("openrouter fallback decode failed"));
    assert!(!error.message.contains("openai-compatible decode failed"));
}

#[test]
fn openrouter_decode_tool_call_missing_id_generates_warning_and_synthetic_id() {
    let response = response::decode_response_json(
        json!({
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
        &ResponseFormat::Text,
    )
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
    let response = response::decode_response_json(
        json!({
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
        &ResponseFormat::Text,
    )
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
    let response = response::decode_response_json(
        json!({
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
        &ResponseFormat::Text,
    )
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
    let response = response::decode_response_json(
        json!({
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
        &ResponseFormat::Text,
    )
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
    let response = response::decode_response_json(
        json!({
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
        &ResponseFormat::Text,
    )
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
    let response = response::decode_response_json(
        json!({
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
        &ResponseFormat::JsonObject,
    )
    .expect("decode should succeed");

    assert!(response.output.structured_output.is_none());
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openrouter.decode.structured_output_parse_failed")
    );
}
