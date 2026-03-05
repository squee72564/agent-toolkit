use std::collections::BTreeMap;

use serde_json::json;

use agent_core::types::{
    ContentPart, FinishReason, Message, MessageRole, Request, ResponseFormat, ToolCall, ToolChoice,
    ToolDefinition, ToolResult, ToolResultContent,
};

use super::decode::{
    decode_anthropic_response, format_anthropic_error_message, parse_anthropic_error_value,
};
use super::encode::encode_anthropic_request;
use super::{AnthropicDecodeEnvelope, AnthropicSpecError, AnthropicSpecErrorKind};

fn base_request(messages: Vec<Message>) -> Request {
    Request {
        model_id: "claude-sonnet-4.6".to_string(),
        messages,
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
fn encode_basic_text_message() {
    let request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);

    let encoded = encode_anthropic_request(&request).expect("encode should succeed");

    assert_eq!(encoded.body["model"], json!("claude-sonnet-4.6"));
    assert_eq!(encoded.body["max_tokens"], json!(1024));
    assert_eq!(encoded.body["messages"][0]["role"], json!("user"));
    assert_eq!(
        encoded.body["messages"][0]["content"][0]["type"],
        json!("text")
    );
}

#[test]
fn encode_system_prefix_mapping() {
    let request = base_request(vec![
        Message {
            role: MessageRole::System,
            content: vec![ContentPart::Text {
                text: "sys-a".to_string(),
            }],
        },
        Message {
            role: MessageRole::System,
            content: vec![ContentPart::Text {
                text: "sys-b".to_string(),
            }],
        },
        Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "hello".to_string(),
            }],
        },
    ]);

    let encoded = encode_anthropic_request(&request).expect("encode should succeed");
    assert_eq!(encoded.body["system"][0]["text"], json!("sys-a"));
    assert_eq!(encoded.body["system"][1]["text"], json!("sys-b"));
    assert_eq!(encoded.body["messages"][0]["role"], json!("user"));
}

#[test]
fn encode_tools_and_tool_choice_mappings() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.tools = vec![ToolDefinition {
        name: "calculator".to_string(),
        description: Some("compute expression".to_string()),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "expression": {"type": "string"}
            },
            "required": ["expression"]
        }),
    }];

    request.tool_choice = ToolChoice::None;
    assert_eq!(
        encode_anthropic_request(&request)
            .expect("encode none")
            .body
            .pointer("/tool_choice/type"),
        Some(&json!("none"))
    );

    request.tool_choice = ToolChoice::Auto;
    assert_eq!(
        encode_anthropic_request(&request)
            .expect("encode auto")
            .body
            .pointer("/tool_choice/type"),
        Some(&json!("auto"))
    );

    request.tool_choice = ToolChoice::Required;
    assert_eq!(
        encode_anthropic_request(&request)
            .expect("encode required")
            .body
            .pointer("/tool_choice/type"),
        Some(&json!("any"))
    );

    request.tool_choice = ToolChoice::Specific {
        name: "calculator".to_string(),
    };
    let encoded = encode_anthropic_request(&request).expect("encode specific");
    assert_eq!(
        encoded.body.pointer("/tool_choice/type"),
        Some(&json!("tool"))
    );
    assert_eq!(
        encoded
            .body
            .pointer("/tool_choice/disable_parallel_tool_use"),
        Some(&json!(true))
    );
    assert_eq!(
        encoded.body.pointer("/tools/0/name"),
        Some(&json!("calculator"))
    );
    assert_eq!(
        encoded.body.pointer("/tools/0/input_schema/type"),
        Some(&json!("object"))
    );
}

#[test]
fn encode_tool_call_and_tool_result_sequencing_success() {
    let request = base_request(vec![
        Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "compute 2+2".to_string(),
            }],
        },
        Message {
            role: MessageRole::Assistant,
            content: vec![ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: "call_1".to_string(),
                    name: "calculator".to_string(),
                    arguments_json: json!({"expression":"2+2"}),
                },
            }],
        },
        Message {
            role: MessageRole::Tool,
            content: vec![ContentPart::ToolResult {
                tool_result: ToolResult {
                    tool_call_id: "call_1".to_string(),
                    content: ToolResultContent::Text {
                        text: "4".to_string(),
                    },
                    raw_provider_content: None,
                },
            }],
        },
        Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "thanks".to_string(),
            }],
        },
    ]);

    let encoded = encode_anthropic_request(&request).expect("encode should succeed");
    let messages = encoded.body["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages[1]["role"], json!("assistant"));
    assert_eq!(messages[1]["content"][0]["type"], json!("tool_use"));
    assert_eq!(messages[2]["role"], json!("user"));
    assert_eq!(messages[2]["content"][0]["type"], json!("tool_result"));
}

#[test]
fn encode_rejects_non_prefix_system_message() {
    let request = base_request(vec![
        Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "hello".to_string(),
            }],
        },
        Message {
            role: MessageRole::System,
            content: vec![ContentPart::Text {
                text: "late system".to_string(),
            }],
        },
    ]);

    let error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Validation);
    assert!(error.message().contains("contiguous prefix"));
}

#[test]
fn encode_rejects_bad_tool_choice() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.tool_choice = ToolChoice::Specific {
        name: "missing".to_string(),
    };

    let error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Validation);
    assert!(
        error
            .message()
            .contains("requires at least one tool definition")
    );
}

#[test]
fn encode_rejects_invalid_tool_schema_or_name() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.tools = vec![ToolDefinition {
        name: "".to_string(),
        description: None,
        parameters_schema: json!({"type":"object"}),
    }];
    let empty_name_error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(empty_name_error.kind(), AnthropicSpecErrorKind::Validation);
    assert!(empty_name_error.message().contains("non-empty names"));

    request.tools = vec![ToolDefinition {
        name: "tool".to_string(),
        description: None,
        parameters_schema: json!("not-object"),
    }];
    let bad_schema_error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(bad_schema_error.kind(), AnthropicSpecErrorKind::Validation);
    assert!(bad_schema_error.message().contains("must be a JSON object"));
}

#[test]
fn encode_rejects_tool_result_before_tool_call() {
    let request = base_request(vec![
        Message {
            role: MessageRole::Tool,
            content: vec![ContentPart::ToolResult {
                tool_result: ToolResult {
                    tool_call_id: "call_missing".to_string(),
                    content: ToolResultContent::Text {
                        text: "done".to_string(),
                    },
                    raw_provider_content: None,
                },
            }],
        },
        Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "hello".to_string(),
            }],
        },
    ]);

    let error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::ProtocolViolation);
    assert!(error.message().contains("unknown tool_call_id"));
}

#[test]
fn encode_rejects_empty_tool_call_id() {
    let request = base_request(vec![Message {
        role: MessageRole::Assistant,
        content: vec![ContentPart::ToolCall {
            tool_call: ToolCall {
                id: "   ".to_string(),
                name: "calculator".to_string(),
                arguments_json: json!({"expression":"2+2"}),
            },
        }],
    }]);

    let error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Validation);
    assert!(error.message().contains("non-empty tool_call id"));
}

#[test]
fn encode_rejects_empty_tool_call_name() {
    let request = base_request(vec![Message {
        role: MessageRole::Assistant,
        content: vec![ContentPart::ToolCall {
            tool_call: ToolCall {
                id: "call_1".to_string(),
                name: "  ".to_string(),
                arguments_json: json!({"expression":"2+2"}),
            },
        }],
    }]);

    let error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Validation);
    assert!(error.message().contains("non-empty tool_call name"));
}

#[test]
fn encode_rejects_empty_tool_result_tool_call_id() {
    let request = base_request(vec![
        Message {
            role: MessageRole::Assistant,
            content: vec![ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: "call_1".to_string(),
                    name: "calculator".to_string(),
                    arguments_json: json!({"expression":"2+2"}),
                },
            }],
        },
        Message {
            role: MessageRole::Tool,
            content: vec![ContentPart::ToolResult {
                tool_result: ToolResult {
                    tool_call_id: "  ".to_string(),
                    content: ToolResultContent::Text {
                        text: "4".to_string(),
                    },
                    raw_provider_content: None,
                },
            }],
        },
    ]);

    let error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Validation);
    assert!(error.message().contains("non-empty tool_call_id"));
}

#[test]
fn encode_rejects_duplicate_tool_call_ids() {
    let request = base_request(vec![Message {
        role: MessageRole::Assistant,
        content: vec![
            ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: "call_1".to_string(),
                    name: "calculator".to_string(),
                    arguments_json: json!({"expression":"2+2"}),
                },
            },
            ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: "call_1".to_string(),
                    name: "calculator_v2".to_string(),
                    arguments_json: json!({"expression":"5+5"}),
                },
            },
        ],
    }]);

    let error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::ProtocolViolation);
    assert!(
        error
            .message()
            .contains("duplicate assistant tool_call id 'call_1'")
    );
}

#[test]
fn encode_rejects_structured_output_with_assistant_prefill() {
    let mut request = base_request(vec![
        Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "give me JSON".to_string(),
            }],
        },
        Message {
            role: MessageRole::Assistant,
            content: vec![ContentPart::Text {
                text: "{".to_string(),
            }],
        },
    ]);
    request.response_format = ResponseFormat::JsonObject;

    let error = encode_anthropic_request(&request).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Validation);
    assert!(error.message().contains("assistant-prefill"));
}

#[test]
fn encode_emits_warning_when_temperature_and_top_p_set() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.temperature = Some(0.3);
    request.top_p = Some(0.9);

    let encoded = encode_anthropic_request(&request).expect("encode should succeed");
    assert!(
        encoded
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.encode.both_temperature_and_top_p_set")
    );
}

#[test]
fn encode_emits_warning_when_default_max_tokens_applied() {
    let request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    let encoded = encode_anthropic_request(&request).expect("encode should succeed");

    assert_eq!(encoded.body["max_tokens"], json!(1024));
    assert!(
        encoded
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.encode.default_max_tokens_applied")
    );
}

#[test]
fn encode_emits_warning_when_dropping_unsupported_metadata_keys() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request
        .metadata
        .insert("user_id".to_string(), "user-1".to_string());
    request
        .metadata
        .insert("trace_id".to_string(), "trace-123".to_string());

    let encoded = encode_anthropic_request(&request).expect("encode should succeed");
    assert_eq!(encoded.body["metadata"], json!({"user_id":"user-1"}));
    assert!(
        encoded
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.encode.dropped_unsupported_metadata_keys")
    );
}

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
    match &response.output.content[0] {
        ContentPart::ToolCall { tool_call } => {
            assert_eq!(tool_call.id, "call_1");
            assert_eq!(tool_call.name, "calculator");
            assert_eq!(tool_call.arguments_json, json!({"expression":"2+2"}));
        }
        other => panic!("expected tool call content part, got {other:?}"),
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
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Decode);
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
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Decode);
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
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Decode);
    assert!(error.message().contains("must be a JSON object"));
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
    assert_eq!(error.kind(), AnthropicSpecErrorKind::Upstream);
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
    let encode_error = encode_anthropic_request(&request).expect_err("encode should fail");
    match encode_error {
        AnthropicSpecError::Validation { .. } | AnthropicSpecError::ProtocolViolation { .. } => {}
        other => panic!("expected validation/protocol error, got {other:?}"),
    }

    let payload = AnthropicDecodeEnvelope {
        body: json!({
            "role": "user",
            "content": [],
            "stop_reason": "end_turn"
        }),
        requested_response_format: ResponseFormat::Text,
    };
    let decode_error = decode_anthropic_response(&payload).expect_err("decode should fail");
    assert_eq!(decode_error.kind(), AnthropicSpecErrorKind::Decode);
}

#[test]
fn anthropic_spec_error_constructors_set_kind_and_message() {
    let validation_error = AnthropicSpecError::validation("bad validation");
    assert_eq!(validation_error.kind(), AnthropicSpecErrorKind::Validation);
    assert_eq!(validation_error.message(), "bad validation");

    let protocol_error = AnthropicSpecError::protocol_violation("bad protocol");
    assert_eq!(
        protocol_error.kind(),
        AnthropicSpecErrorKind::ProtocolViolation
    );
    assert_eq!(protocol_error.message(), "bad protocol");

    let decode_error = AnthropicSpecError::decode("bad decode");
    assert_eq!(decode_error.kind(), AnthropicSpecErrorKind::Decode);
    assert_eq!(decode_error.message(), "bad decode");

    let upstream_error = AnthropicSpecError::upstream("bad upstream");
    assert_eq!(upstream_error.kind(), AnthropicSpecErrorKind::Upstream);
    assert_eq!(upstream_error.message(), "bad upstream");
}

#[test]
fn anthropic_spec_error_encode_with_source_preserves_source_chain() {
    let encode_error =
        AnthropicSpecError::encode_with_source("failed to encode", std::io::Error::other("io"));
    assert_eq!(encode_error.kind(), AnthropicSpecErrorKind::Encode);
    assert_eq!(encode_error.message(), "failed to encode");

    let source = std::error::Error::source(&encode_error).expect("source should exist");
    assert!(source.to_string().contains("io"));
}

#[test]
fn anthropic_spec_error_decode_variant_with_source_exposes_source() {
    let decode_error = AnthropicSpecError::Decode {
        message: "failed to decode".to_string(),
        source: Some(Box::new(std::io::Error::other("wire"))),
    };
    assert_eq!(decode_error.kind(), AnthropicSpecErrorKind::Decode);
    assert_eq!(decode_error.message(), "failed to decode");

    let source = std::error::Error::source(&decode_error).expect("source should exist");
    assert!(source.to_string().contains("wire"));
}

#[test]
fn anthropic_spec_error_unsupported_feature_kind_and_message() {
    let error = AnthropicSpecError::unsupported_feature("streaming tools");
    assert_eq!(error.kind(), AnthropicSpecErrorKind::UnsupportedFeature);
    assert_eq!(error.message(), "streaming tools");
}
