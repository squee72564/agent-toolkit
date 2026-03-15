use std::io;

use serde_json::json;

use agent_core::types::{ContentPart, ResponseFormat};

use crate::openai_family::decode::decode_openai_response;
use crate::openai_family::{OpenAiDecodeEnvelope, OpenAiFamilyError, OpenAiFamilyErrorKind};

#[test]
fn decode_top_level_error_maps_to_upstream() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "error": {
                "message": "Bad API key",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_openai_response(&envelope).expect_err("decode should fail");
    assert_eq!(error.kind(), OpenAiFamilyErrorKind::Upstream);
    assert!(error.message().contains("openai error:"));
    assert!(error.message().contains("invalid_api_key"));
}

#[test]
fn decode_in_progress_status_uses_interpolated_message() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "in_progress",
            "model": "gpt-4.1-mini",
            "output": []
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_openai_response(&envelope).expect_err("decode should fail");

    match error {
        OpenAiFamilyError::Decode { message, .. } => {
            assert!(message.contains("in_progress"));
            assert!(!message.contains("{status}"));
        }
        _ => panic!("expected decode error variant"),
    }
}

#[test]
fn decode_unknown_output_item_is_ignored_with_warning() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "gpt-4.1-mini",
            "output": [
                {
                    "type": "message",
                    "content": [
                        { "type": "output_text", "text": "hello" }
                    ]
                },
                {
                    "type": "new_item_type",
                    "payload": { "value": 1 }
                }
            ]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_openai_response(&envelope).expect("decode should succeed");
    assert_eq!(response.output.content.len(), 1);
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openai.decode.unknown_output_item")
    );
}

#[test]
fn decode_unknown_message_part_is_ignored_with_warning() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "gpt-4.1-mini",
            "output": [
                {
                    "type": "message",
                    "content": [
                        { "type": "output_text", "text": "hello" },
                        { "type": "audio", "url": "https://example.com/a.wav" }
                    ]
                }
            ]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_openai_response(&envelope).expect("decode should succeed");
    assert_eq!(response.output.content.len(), 1);
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openai.decode.unknown_message_part")
    );
}

#[test]
fn decode_invalid_tool_call_arguments_falls_back_to_string_with_warning() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "gpt-4.1-mini",
            "output": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "lookup_weather",
                    "arguments": "{invalid-json"
                }
            ]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_openai_response(&envelope).expect("decode should succeed");
    assert_eq!(
        response.finish_reason,
        agent_core::types::FinishReason::ToolCalls
    );
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openai.decode.invalid_tool_call_arguments")
    );

    let first = response
        .output
        .content
        .first()
        .expect("content should include tool call");
    match first {
        ContentPart::ToolCall { tool_call } => {
            assert_eq!(tool_call.name, "lookup_weather");
            assert_eq!(tool_call.arguments_json, json!("{invalid-json"));
        }
        _ => panic!("expected first output content part to be a tool call"),
    }
}

#[test]
fn decode_function_call_rejects_blank_call_id() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "gpt-4.1-mini",
            "output": [
                {
                    "type": "function_call",
                    "call_id": "   ",
                    "name": "lookup_weather",
                    "arguments": "{\"city\":\"SF\"}"
                }
            ]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_openai_response(&envelope).expect_err("decode should fail");
    match error {
        OpenAiFamilyError::Decode { message, .. } => {
            assert!(message.contains("call_id must not be empty"));
        }
        _ => panic!("expected decode error variant"),
    }
}

#[test]
fn decode_function_call_rejects_blank_name() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "gpt-4.1-mini",
            "output": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": " ",
                    "arguments": "{\"city\":\"SF\"}"
                }
            ]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_openai_response(&envelope).expect_err("decode should fail");
    match error {
        OpenAiFamilyError::Decode { message, .. } => {
            assert!(message.contains("name must not be empty"));
        }
        _ => panic!("expected decode error variant"),
    }
}

#[test]
fn decode_function_call_rejects_blank_arguments() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "gpt-4.1-mini",
            "output": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "lookup_weather",
                    "arguments": "  "
                }
            ]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_openai_response(&envelope).expect_err("decode should fail");
    match error {
        OpenAiFamilyError::Decode { message, .. } => {
            assert!(message.contains("arguments must not be empty"));
        }
        _ => panic!("expected decode error variant"),
    }
}

#[test]
fn decode_refusal_whitespace_only_is_ignored() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "gpt-4.1-mini",
            "output": [
                {
                    "type": "refusal",
                    "text": "   "
                }
            ]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_openai_response(&envelope).expect("decode should succeed");
    assert!(response.output.content.is_empty());
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.code == "openai.decode.empty_content")
    );
}

#[test]
fn decode_refusal_text_is_trimmed_and_emitted() {
    let envelope = OpenAiDecodeEnvelope {
        body: json!({
            "status": "completed",
            "model": "gpt-4.1-mini",
            "output": [
                {
                    "type": "refusal",
                    "refusal": "  cannot comply  "
                }
            ]
        }),
        requested_response_format: ResponseFormat::Text,
    };

    let response = decode_openai_response(&envelope).expect("decode should succeed");
    assert_eq!(response.output.content.len(), 1);
    match &response.output.content[0] {
        ContentPart::Text { text } => assert_eq!(text, "cannot comply"),
        _ => panic!("expected text content part"),
    }
}

#[test]
fn error_kind_maps_for_all_variants() {
    let validation = OpenAiFamilyError::validation("invalid input");
    assert_eq!(validation.kind(), OpenAiFamilyErrorKind::Validation);

    let encode = OpenAiFamilyError::encode_with_source("encode failed", io::Error::other("boom"));
    assert_eq!(encode.kind(), OpenAiFamilyErrorKind::Encode);

    let decode = OpenAiFamilyError::decode("decode failed");
    assert_eq!(decode.kind(), OpenAiFamilyErrorKind::Decode);

    let upstream = OpenAiFamilyError::upstream("upstream failed");
    assert_eq!(upstream.kind(), OpenAiFamilyErrorKind::Upstream);

    let protocol = OpenAiFamilyError::protocol_violation("protocol failed");
    assert_eq!(protocol.kind(), OpenAiFamilyErrorKind::ProtocolViolation);

    let unsupported = OpenAiFamilyError::unsupported_feature("unsupported feature");
    assert_eq!(
        unsupported.kind(),
        OpenAiFamilyErrorKind::UnsupportedFeature
    );
}

#[test]
fn error_message_returns_original_message_for_all_variants() {
    assert_eq!(
        OpenAiFamilyError::validation("invalid input").message(),
        "invalid input"
    );
    assert_eq!(
        OpenAiFamilyError::encode_with_source("encode failed", io::Error::other("boom")).message(),
        "encode failed"
    );
    assert_eq!(
        OpenAiFamilyError::decode("decode failed").message(),
        "decode failed"
    );
    assert_eq!(
        OpenAiFamilyError::upstream("upstream failed").message(),
        "upstream failed"
    );
    assert_eq!(
        OpenAiFamilyError::protocol_violation("protocol failed").message(),
        "protocol failed"
    );
    assert_eq!(
        OpenAiFamilyError::unsupported_feature("unsupported feature").message(),
        "unsupported feature"
    );
}

#[test]
fn encode_with_source_preserves_error_chain() {
    let error = OpenAiFamilyError::encode_with_source("encode failed", io::Error::other("boom"));

    let source = std::error::Error::source(&error).expect("source should be present");
    assert!(
        source.to_string().contains("boom"),
        "source message should include original io error message"
    );
}

#[test]
fn decode_constructor_has_no_source() {
    let error = OpenAiFamilyError::decode("decode failed");
    assert!(
        std::error::Error::source(&error).is_none(),
        "decode constructor should not attach a source"
    );
}
