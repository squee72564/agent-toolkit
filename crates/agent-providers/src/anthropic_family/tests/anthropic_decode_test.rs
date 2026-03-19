use serde_json::json;

use agent_core::types::{ContentPart, FinishReason, ResponseFormat};

use super::anthropic_test_helpers::*;
use crate::anthropic_family::decode::{
    decode_anthropic_response, format_anthropic_error_message, parse_anthropic_error_value,
};
use crate::anthropic_family::{
    AnthropicDecodeEnvelope, AnthropicFamilyError, AnthropicFamilyErrorKind,
};

#[test]
fn decode_basic_text_usage_and_stop_reason() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"hello"}],
            "usage": {
                "input_tokens": 10,
                "cache_creation_input_tokens": 2,
                "cache_read_input_tokens": 3,
                "output_tokens": 7
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert_eq!(response.model, "claude-sonnet-4.6");
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert_eq!(response.usage.input_tokens, Some(15));
    assert_eq!(response.usage.cached_input_tokens, Some(3));
    assert_eq!(response.usage.output_tokens, Some(7));
    assert_eq!(response.usage.total_tokens, Some(22));
}

#[test]
fn decode_tool_use_mapping() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "tool_use",
            "content": [{
                "type":"tool_use",
                "id":"call_1",
                "name":"calculator",
                "input":{"expression":"2+2"}
            }],
            "usage": {
                "input_tokens": 1,
                "output_tokens": 1
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert_eq!(response.finish_reason, FinishReason::ToolCalls);
    assert_eq!(response.output.content.len(), 1);
    assert!(matches!(
        &response.output.content[0],
        ContentPart::ToolCall { .. }
    ));
    if let ContentPart::ToolCall { tool_call } = &response.output.content[0] {
        assert_eq!(tool_call.id, "call_1");
        assert_eq!(tool_call.name, "calculator");
        assert_eq!(tool_call.arguments_json, json!({"expression":"2+2"}));
    }
}

#[test]
fn decode_structured_output_for_json_object_and_json_schema() {
    let json_object_payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [
                {"type":"text","text":"not-json"},
                {"type":"text","text":"{\"ok\":true}"}
            ],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::JsonObject,
    };

    let json_object_response =
        decode_anthropic_response(&json_object_payload).expect("decode should succeed");
    assert_eq!(
        json_object_response.output.structured_output,
        Some(json!({"ok":true}))
    );

    let json_schema_payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [
                {"type":"text","text":"{\"value\":123}"},
                {"type":"text","text":"{\"ignored\":true}"}
            ],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::JsonSchema {
            name: "shape".to_string(),
            schema: json!({"type":"object"}),
        },
    };

    let json_schema_response =
        decode_anthropic_response(&json_schema_payload).expect("decode should succeed");
    assert_eq!(
        json_schema_response.output.structured_output,
        Some(json!({"value":123}))
    );
}

#[test]
fn decode_rejects_malformed_payload() {
    let payload = AnthropicDecodeEnvelope {
        body: json!(["not-an-object"]),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Decode);
    assert!(error.message().contains("JSON object"));
}

#[test]
fn decode_rejects_missing_required_fields() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "content": []
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Decode);
    assert!(error.message().contains("missing stop_reason"));
}

#[test]
fn decode_rejects_non_object_tool_use_input() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "tool_use",
            "content": [{
                "type":"tool_use",
                "id":"call_1",
                "name":"calculator",
                "input":"not-object"
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Decode);
    assert!(error.message().contains("must be a JSON object"));
}

#[test]
fn decode_unknown_content_block_warns_and_maps_to_text() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{
                "type":"future_block",
                "z": 1,
                "a": {"k":"v"}
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert_eq!(response.output.content.len(), 1);
    assert!(matches!(
        &response.output.content[0],
        ContentPart::Text { .. }
    ));
    if let ContentPart::Text { text } = &response.output.content[0] {
        assert_eq!(text, r#"{"a":{"k":"v"},"type":"future_block","z":1}"#);
    }
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.unknown_content_block_mapped_to_text")
    );
}

#[test]
fn decode_thinking_block_warns_and_skips_block() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"thinking","text":"hidden"}],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert!(response.output.content.is_empty());
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.unrepresentable_thinking_skipped")
    );
}

#[test]
fn decode_redacted_thinking_block_warns_and_skips_block() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"redacted_thinking","data":"hidden"}],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert!(response.output.content.is_empty());
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.unrepresentable_thinking_skipped")
    );
}

#[test]
fn decode_json_object_extracts_from_combined_text_fallback() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [
                {"type":"text","text":"lead-in"},
                {"type":"text","text":"prefix {\"ok\":true,\"nested\":{\"value\":1}} suffix"}
            ],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::JsonObject,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert_eq!(
        response.output.structured_output,
        Some(json!({"ok":true,"nested":{"value":1}}))
    );
}

#[test]
fn decode_rejects_non_object_usage() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"ok"}],
            "usage": "invalid"
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Decode);
    assert!(error.message().contains("usage must be a JSON object"));
}

#[test]
fn decode_rejects_non_numeric_usage_field() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"ok"}],
            "usage": {
                "input_tokens": "five",
                "output_tokens": 1
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Decode);
    assert!(error.message().contains("must be numeric"));
}

#[test]
fn decode_rejects_signed_usage_field() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"ok"}],
            "usage": {
                "input_tokens": -1,
                "output_tokens": 1
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Decode);
    assert!(error.message().contains("must be an unsigned integer"));
}

#[test]
fn decode_maps_known_stop_reasons() {
    let cases = vec![
        ("stop_sequence", FinishReason::Stop),
        ("max_tokens", FinishReason::Length),
        ("refusal", FinishReason::ContentFilter),
        ("pause_turn", FinishReason::Other),
    ];

    for (stop_reason, expected) in cases {
        let payload = AnthropicDecodeEnvelope {
            body: json!({
                "role": "assistant",
                "model": "claude-sonnet-4.6",
                "stop_reason": stop_reason,
                "content": [{"type":"text","text":"ok"}],
                "usage": {"input_tokens": 1, "output_tokens": 1}
            }),
            requested_response_format: ResponseFormat::Text,
        };

        let response = decode_anthropic_response(&payload).expect("decode should succeed");
        assert_eq!(response.finish_reason, expected);
        if stop_reason != "pause_turn" {
            assert!(
                !response
                    .warnings
                    .iter()
                    .any(|warning| warning.code == "anthropic.decode.unknown_stop_reason")
            );
        }
    }
}

#[test]
fn decode_unknown_stop_reason_warns_and_maps_to_other() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "future_reason",
            "content": [{"type":"text","text":"hello"}],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert_eq!(response.finish_reason, FinishReason::Other);
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.unknown_stop_reason")
    );
}

#[test]
fn decode_missing_and_partial_usage_warns() {
    let missing_usage_payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"ok"}]
        }),
        requested_response_format: ResponseFormat::Text,
    };
    let missing_usage =
        decode_anthropic_response(&missing_usage_payload).expect("decode should succeed");
    assert_eq!(missing_usage.usage, agent_core::types::Usage::default());
    assert!(
        missing_usage
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.usage_missing")
    );

    let partial_usage_payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"ok"}],
            "usage": {"input_tokens": 4}
        }),
        requested_response_format: ResponseFormat::Text,
    };
    let partial_usage =
        decode_anthropic_response(&partial_usage_payload).expect("decode should succeed");
    assert_eq!(partial_usage.usage.input_tokens, Some(4));
    assert_eq!(partial_usage.usage.output_tokens, None);
    assert!(
        partial_usage
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.usage_partial")
    );
}

#[test]
fn decode_usage_billed_input_overflow_warns_and_drops_aggregate() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"ok"}],
            "usage": {
                "input_tokens": u64::MAX,
                "cache_creation_input_tokens": 1,
                "output_tokens": 1
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert_eq!(response.usage.input_tokens, None);
    assert_eq!(response.usage.output_tokens, Some(1));
    assert_eq!(response.usage.total_tokens, None);
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.usage_overflow")
    );
}

#[test]
fn decode_usage_total_tokens_overflow_warns_and_drops_total() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"ok"}],
            "usage": {
                "input_tokens": 5,
                "output_tokens": u64::MAX
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert_eq!(response.usage.input_tokens, Some(5));
    assert_eq!(response.usage.output_tokens, Some(u64::MAX));
    assert_eq!(response.usage.total_tokens, None);
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.usage_overflow")
    );
}

#[test]
fn decode_top_level_upstream_error_parsing_and_formatting() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "message": "messages: Input should be a valid list"
            },
            "request_id": "req_123"
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Upstream);
    assert!(error.message().contains("anthropic error:"));
    assert!(error.message().contains("type=invalid_request_error"));
    assert!(error.message().contains("request_id=req_123"));

    let root = payload
        .body
        .as_object()
        .expect("error payload should be object");
    let envelope = parse_anthropic_error_value(root).expect("expected parsed envelope");
    assert_eq!(envelope.message, "messages: Input should be a valid list");
    assert_eq!(
        format_anthropic_error_message(&envelope),
        "anthropic error: messages: Input should be a valid list [type=invalid_request_error, request_id=req_123]"
    );
}

#[test]
fn decode_empty_output_warns() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert!(response.output.content.is_empty());
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.empty_output")
    );
}

#[test]
fn decode_structured_output_parse_failure_warns() {
    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "assistant",
            "model": "claude-sonnet-4.6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"{not-json"}],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        requested_response_format: ResponseFormat::JsonSchema {
            name: "shape".to_string(),
            schema: json!({"type":"object"}),
        },
    };

    let response = decode_anthropic_response(&payload).expect("decode should succeed");
    assert_eq!(response.output.structured_output, None);
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.decode.structured_output_parse_failed")
    );
}

#[test]
fn encode_and_decode_error_variant_smoke() {
    let request = base_request(vec![]);
    let encode_error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert!(matches!(
        encode_error,
        AnthropicFamilyError::Validation { .. } | AnthropicFamilyError::ProtocolViolation { .. }
    ));

    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "user",
            "content": [],
            "stop_reason": "end_turn"
        }),
        requested_response_format: ResponseFormat::Text,
    };
    let decode_error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(decode_error.kind(), AnthropicFamilyErrorKind::Decode);
}

#[test]
fn anthropic_family_error_constructors_set_kind_and_message() {
    let validation_error = AnthropicFamilyError::validation("bad validation");
    assert_eq!(
        validation_error.kind(),
        AnthropicFamilyErrorKind::Validation
    );
    assert_eq!(validation_error.message(), "bad validation");

    let protocol_error = AnthropicFamilyError::protocol_violation("bad protocol");
    assert_eq!(
        protocol_error.kind(),
        AnthropicFamilyErrorKind::ProtocolViolation
    );
    assert_eq!(protocol_error.message(), "bad protocol");

    let decode_error = AnthropicFamilyError::decode("bad decode");
    assert_eq!(decode_error.kind(), AnthropicFamilyErrorKind::Decode);
    assert_eq!(decode_error.message(), "bad decode");

    let upstream_error = AnthropicFamilyError::upstream("bad upstream");
    assert_eq!(upstream_error.kind(), AnthropicFamilyErrorKind::Upstream);
    assert_eq!(upstream_error.message(), "bad upstream");
}

#[test]
fn anthropic_family_error_encode_with_source_preserves_source_chain() {
    let encode_error =
        AnthropicFamilyError::encode_with_source("failed to encode", std::io::Error::other("io"));
    assert_eq!(encode_error.kind(), AnthropicFamilyErrorKind::Encode);
    assert_eq!(encode_error.message(), "failed to encode");

    let source = std::error::Error::source(&encode_error).expect("source should exist");
    assert!(source.to_string().contains("io"));
}

#[test]
fn anthropic_family_error_decode_variant_with_source_exposes_source() {
    let decode_error = AnthropicFamilyError::Decode {
        message: "failed to decode".to_string(),
        source: Some(Box::new(std::io::Error::other("wire"))),
    };
    assert_eq!(decode_error.kind(), AnthropicFamilyErrorKind::Decode);
    assert_eq!(decode_error.message(), "failed to decode");

    let source = std::error::Error::source(&decode_error).expect("source should exist");
    assert!(source.to_string().contains("wire"));
}

#[test]
fn anthropic_family_error_unsupported_feature_kind_and_message() {
    let error = AnthropicFamilyError::unsupported_feature("streaming tools");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::UnsupportedFeature);
    assert_eq!(error.message(), "streaming tools");
}
