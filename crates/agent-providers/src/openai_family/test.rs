use std::collections::BTreeMap;
use std::io;

use serde_json::json;

use agent_core::types::{
    ContentPart, Message, MessageRole, Request, ResponseFormat, ToolCall, ToolChoice,
    ToolDefinition, ToolResult, ToolResultContent,
};

use super::OpenAiSpecError;
use super::decode::decode_openai_response;
use super::encode::encode_openai_request;
use super::schema_rules::{canonicalize_json, is_strict_compatible_schema, stable_json_string};
use super::{OpenAiDecodeEnvelope, OpenAiSpecErrorKind};

fn base_request(messages: Vec<Message>) -> Request {
    Request {
        model_id: "gpt-4.1-mini".to_string(),
        stream: false,
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
fn encode_simple_user_text_message() {
    let request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);

    let encoded = encode_openai_request(request.clone()).expect("encoding should succeed");

    assert_eq!(encoded.body["model"], json!("gpt-4.1-mini"));
    assert_eq!(encoded.body["text"]["format"]["type"], json!("text"));
    assert_eq!(encoded.body["input"].as_array().map(Vec::len), Some(1));
}

#[test]
fn encode_warnings_empty_for_basic_request() {
    let request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);

    let encoded = encode_openai_request(request.clone()).expect("encoding should succeed");

    assert!(encoded.warnings.is_empty());
}

#[test]
fn encode_warns_when_top_p_ignored() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.top_p = Some(0.8);

    let encoded = encode_openai_request(request.clone()).expect("encoding should succeed");

    assert!(
        encoded
            .warnings
            .iter()
            .any(|w| w.code == "openai.encode.ignored_top_p")
    );
}

#[test]
fn encode_warns_when_stop_ignored() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.stop = vec!["END".to_string()];

    let encoded = encode_openai_request(request.clone()).expect("encoding should succeed");

    assert!(
        encoded
            .warnings
            .iter()
            .any(|w| w.code == "openai.encode.ignored_stop")
    );
}

#[test]
fn encode_warns_when_tool_schema_not_strict_compatible() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.tools = vec![ToolDefinition {
        name: "lookup_weather".to_string(),
        description: None,
        parameters_schema: json!({
            "anyOf": [
                {
                    "type": "object",
                    "properties": { "city": { "type": "string" } },
                    "required": ["city"],
                    "additionalProperties": false
                }
            ]
        }),
    }];

    let encoded = encode_openai_request(request.clone()).expect("encoding should succeed");

    assert_eq!(encoded.body["tools"][0]["strict"], json!(false));
    assert!(
        encoded
            .warnings
            .iter()
            .any(|w| w.code == "openai.encode.non_strict_tool_schema")
    );
}

#[test]
fn encode_maps_tool_definition_with_description_and_object_schema() {
    let mut request = base_request(vec![Message::user_text("hello")]);
    request.tools = vec![ToolDefinition {
        name: "lookup_weather".to_string(),
        description: Some("Look up forecast details".to_string()),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "city": { "type": "string" }
            },
            "required": ["city"],
            "additionalProperties": false
        }),
    }];

    let encoded = encode_openai_request(request).expect("encoding should succeed");

    assert_eq!(
        encoded.body.pointer("/tools/0/type"),
        Some(&json!("function"))
    );
    assert_eq!(
        encoded.body.pointer("/tools/0/name"),
        Some(&json!("lookup_weather"))
    );
    assert_eq!(
        encoded.body.pointer("/tools/0/description"),
        Some(&json!("Look up forecast details"))
    );
    assert_eq!(
        encoded.body.pointer("/tools/0/parameters/type"),
        Some(&json!("object"))
    );
    assert_eq!(encoded.body.pointer("/tools/0/strict"), Some(&json!(true)));
}

#[test]
fn encode_maps_json_schema_response_format() {
    let mut request = base_request(vec![Message::user_text("return json")]);
    request.response_format = ResponseFormat::JsonSchema {
        name: "result".to_string(),
        schema: json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" }
            },
            "required": ["ok"],
            "additionalProperties": false
        }),
    };

    let encoded = encode_openai_request(request).expect("encoding should succeed");

    assert_eq!(
        encoded.body.pointer("/text/format/type"),
        Some(&json!("json_schema"))
    );
    assert_eq!(
        encoded.body.pointer("/text/format/name"),
        Some(&json!("result"))
    );
    assert_eq!(
        encoded.body.pointer("/text/format/schema"),
        Some(&json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" }
            },
            "required": ["ok"],
            "additionalProperties": false
        }))
    );
    assert_eq!(
        encoded.body.pointer("/text/format/strict"),
        Some(&json!(true))
    );
}

#[test]
fn encode_emits_multiple_warnings_together() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.top_p = Some(0.8);
    request.stop = vec!["END".to_string()];
    request.tools = vec![ToolDefinition {
        name: "lookup_weather".to_string(),
        description: None,
        parameters_schema: json!({
            "anyOf": [
                {
                    "type": "object",
                    "properties": { "city": { "type": "string" } },
                    "required": ["city"],
                    "additionalProperties": false
                }
            ]
        }),
    }];

    let encoded = encode_openai_request(request.clone()).expect("encoding should succeed");

    assert!(
        encoded
            .warnings
            .iter()
            .any(|w| w.code == "openai.encode.ignored_top_p")
    );
    assert!(
        encoded
            .warnings
            .iter()
            .any(|w| w.code == "openai.encode.ignored_stop")
    );
    assert!(
        encoded
            .warnings
            .iter()
            .any(|w| w.code == "openai.encode.non_strict_tool_schema")
    );
}

#[test]
fn reject_specific_tool_choice_when_tool_missing() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hi".to_string(),
        }],
    }]);
    request.tool_choice = ToolChoice::Specific {
        name: "lookup_weather".to_string(),
    };

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    assert!(
        matches!(
            &error,
            OpenAiSpecError::Validation { message }
                if message.contains("requires at least one tool definition")
        ),
        "expected validation error for missing tool definitions, got: {error:?}"
    );
}

#[test]
fn reject_tool_result_without_prior_tool_call() {
    let request = base_request(vec![Message {
        role: MessageRole::Tool,
        content: vec![ContentPart::ToolResult {
            tool_result: ToolResult {
                tool_call_id: "call_123".to_string(),
                content: ToolResultContent::Text {
                    text: "done".to_string(),
                },
                raw_provider_content: None,
            },
        }],
    }]);

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    assert!(
        matches!(
            &error,
            OpenAiSpecError::ProtocolViolation { message }
                if message.contains("tool_result_without_matching_tool_call")
        ),
        "expected protocol violation for unmatched tool result, got: {error:?}"
    );
}

#[test]
fn strict_schema_requires_no_anyof_and_full_required_list() {
    let valid = json!({
        "type": "object",
        "properties": {
            "city": { "type": "string" }
        },
        "required": ["city"],
        "additionalProperties": false
    });

    let invalid = json!({
        "anyOf": [
            {
                "type": "object",
                "properties": { "city": { "type": "string" } },
                "required": ["city"],
                "additionalProperties": false
            }
        ]
    });

    assert!(is_strict_compatible_schema(&valid));
    assert!(!is_strict_compatible_schema(&invalid));
}

#[test]
fn strict_schema_rejects_missing_additional_properties_false() {
    let invalid = json!({
        "type": "object",
        "properties": {
            "city": { "type": "string" }
        },
        "required": ["city"]
    });

    assert!(!is_strict_compatible_schema(&invalid));
}

#[test]
fn strict_schema_rejects_required_with_non_string_entries() {
    let invalid = json!({
        "type": "object",
        "properties": {
            "city": { "type": "string" }
        },
        "required": [123],
        "additionalProperties": false
    });

    assert!(!is_strict_compatible_schema(&invalid));
}

#[test]
fn strict_schema_rejects_duplicate_required_entries() {
    let invalid = json!({
        "type": "object",
        "properties": {
            "city": { "type": "string" }
        },
        "required": ["city", "city"],
        "additionalProperties": false
    });

    assert!(!is_strict_compatible_schema(&invalid));
}

#[test]
fn strict_schema_rejects_required_entries_not_in_properties() {
    let invalid = json!({
        "type": "object",
        "properties": {
            "city": { "type": "string" }
        },
        "required": ["city", "country"],
        "additionalProperties": false
    });

    assert!(!is_strict_compatible_schema(&invalid));
}

#[test]
fn strict_schema_requires_all_properties_in_required() {
    let invalid = json!({
        "type": "object",
        "properties": {
            "city": { "type": "string" },
            "country": { "type": "string" }
        },
        "required": ["city"],
        "additionalProperties": false
    });

    assert!(!is_strict_compatible_schema(&invalid));
}

#[test]
fn strict_schema_accepts_nested_objects_and_array_items() {
    let valid = json!({
        "type": "object",
        "properties": {
            "location": {
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                },
                "required": ["city"],
                "additionalProperties": false
            },
            "tags": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string" }
                    },
                    "required": ["label"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["location", "tags"],
        "additionalProperties": false
    });

    assert!(is_strict_compatible_schema(&valid));
}

#[test]
fn canonicalize_json_sorts_keys_recursively_for_openai_schema_rules() {
    let input = json!({
        "z": {"b": 2, "a": 1},
        "a": [{"d": 4, "c": 3}, 5]
    });

    let canonical = canonicalize_json(&input);
    let as_string = stable_json_string(&canonical);

    assert_eq!(as_string, r#"{"a":[{"c":3,"d":4},5],"z":{"a":1,"b":2}}"#);
}

#[test]
fn serializes_assistant_tool_call_and_tool_result() {
    let request = base_request(vec![
        Message {
            role: MessageRole::Assistant,
            content: vec![ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: "call_weather".to_string(),
                    name: "lookup_weather".to_string(),
                    arguments_json: json!({"city": "SF"}),
                },
            }],
        },
        Message {
            role: MessageRole::Tool,
            content: vec![ContentPart::ToolResult {
                tool_result: ToolResult {
                    tool_call_id: "call_weather".to_string(),
                    content: ToolResultContent::Text {
                        text: "sunny".to_string(),
                    },
                    raw_provider_content: None,
                },
            }],
        },
    ]);

    let encoded = encode_openai_request(request.clone()).expect("encoding should succeed");
    let input = encoded.body["input"]
        .as_array()
        .expect("input must be array");

    assert_eq!(input[0]["type"], json!("function_call"));
    assert_eq!(input[1]["type"], json!("function_call_output"));
}

#[test]
fn reject_empty_model_id() {
    let mut request = base_request(vec![Message::user_text("hello")]);
    request.model_id = "   ".to_string();

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::Validation { message } => {
            assert!(message.contains("model_id must not be empty"));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn reject_json_schema_response_format_with_blank_name() {
    let mut request = base_request(vec![Message::user_text("hello")]);
    request.response_format = ResponseFormat::JsonSchema {
        name: "   ".to_string(),
        schema: json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        }),
    };

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::Validation { message } => {
            assert!(message.contains("requires a non-empty name"));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn reject_json_schema_response_format_with_non_object_schema() {
    let mut request = base_request(vec![Message::user_text("hello")]);
    request.response_format = ResponseFormat::JsonSchema {
        name: "result".to_string(),
        schema: json!("not-an-object"),
    };

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::Validation { message } => {
            assert!(message.contains("schema to be a JSON object"));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn reject_duplicate_tool_names() {
    let mut request = base_request(vec![Message::user_text("hello")]);
    request.tools = vec![
        ToolDefinition {
            name: "lookup_weather".to_string(),
            description: None,
            parameters_schema: json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "lookup_weather".to_string(),
            description: Some("duplicate".to_string()),
            parameters_schema: json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
        },
    ];

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::Validation { message } => {
            assert!(message.contains("duplicate tool definition name"));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn reject_assistant_tool_call_with_blank_id() {
    let request = base_request(vec![Message {
        role: MessageRole::Assistant,
        content: vec![ContentPart::ToolCall {
            tool_call: ToolCall {
                id: "   ".to_string(),
                name: "lookup_weather".to_string(),
                arguments_json: json!({"city": "SF"}),
            },
        }],
    }]);

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::Validation { message } => {
            assert!(message.contains("tool_call id must not be empty"));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn reject_assistant_tool_call_with_blank_name() {
    let request = base_request(vec![Message {
        role: MessageRole::Assistant,
        content: vec![ContentPart::ToolCall {
            tool_call: ToolCall {
                id: "call_weather".to_string(),
                name: "   ".to_string(),
                arguments_json: json!({"city": "SF"}),
            },
        }],
    }]);

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::Validation { message } => {
            assert!(message.contains("tool_call name must not be empty"));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn reject_duplicate_assistant_tool_call_ids() {
    let request = base_request(vec![Message {
        role: MessageRole::Assistant,
        content: vec![
            ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: "call_weather".to_string(),
                    name: "lookup_weather".to_string(),
                    arguments_json: json!({"city": "SF"}),
                },
            },
            ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: "call_weather".to_string(),
                    name: "lookup_weather".to_string(),
                    arguments_json: json!({"city": "SF"}),
                },
            },
        ],
    }]);

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::ProtocolViolation { message } => {
            assert!(message.contains("duplicate_tool_call_id"));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn reject_tool_result_with_blank_tool_call_id() {
    let request = base_request(vec![Message {
        role: MessageRole::Tool,
        content: vec![ContentPart::ToolResult {
            tool_result: ToolResult {
                tool_call_id: "   ".to_string(),
                content: ToolResultContent::Text {
                    text: "done".to_string(),
                },
                raw_provider_content: None,
            },
        }],
    }]);

    let error = encode_openai_request(request.clone()).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::Validation { message } => {
            assert!(message.contains("tool_result tool_call_id must not be empty"));
        }
        _ => panic!("unexpected error variant"),
    }
}

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
    assert_eq!(error.kind(), OpenAiSpecErrorKind::Upstream);
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
        OpenAiSpecError::Decode { message, .. } => {
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
        OpenAiSpecError::Decode { message, .. } => {
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
        OpenAiSpecError::Decode { message, .. } => {
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
        OpenAiSpecError::Decode { message, .. } => {
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
    let validation = OpenAiSpecError::validation("invalid input");
    assert_eq!(validation.kind(), OpenAiSpecErrorKind::Validation);

    let encode = OpenAiSpecError::encode_with_source("encode failed", io::Error::other("boom"));
    assert_eq!(encode.kind(), OpenAiSpecErrorKind::Encode);

    let decode = OpenAiSpecError::decode("decode failed");
    assert_eq!(decode.kind(), OpenAiSpecErrorKind::Decode);

    let upstream = OpenAiSpecError::upstream("upstream failed");
    assert_eq!(upstream.kind(), OpenAiSpecErrorKind::Upstream);

    let protocol = OpenAiSpecError::protocol_violation("protocol failed");
    assert_eq!(protocol.kind(), OpenAiSpecErrorKind::ProtocolViolation);

    let unsupported = OpenAiSpecError::unsupported_feature("unsupported feature");
    assert_eq!(unsupported.kind(), OpenAiSpecErrorKind::UnsupportedFeature);
}

#[test]
fn error_message_returns_original_message_for_all_variants() {
    assert_eq!(
        OpenAiSpecError::validation("invalid input").message(),
        "invalid input"
    );
    assert_eq!(
        OpenAiSpecError::encode_with_source("encode failed", io::Error::other("boom")).message(),
        "encode failed"
    );
    assert_eq!(
        OpenAiSpecError::decode("decode failed").message(),
        "decode failed"
    );
    assert_eq!(
        OpenAiSpecError::upstream("upstream failed").message(),
        "upstream failed"
    );
    assert_eq!(
        OpenAiSpecError::protocol_violation("protocol failed").message(),
        "protocol failed"
    );
    assert_eq!(
        OpenAiSpecError::unsupported_feature("unsupported feature").message(),
        "unsupported feature"
    );
}

#[test]
fn encode_with_source_preserves_error_chain() {
    let error = OpenAiSpecError::encode_with_source("encode failed", io::Error::other("boom"));

    let source = std::error::Error::source(&error).expect("source should be present");
    assert!(
        source.to_string().contains("boom"),
        "source message should include original io error message"
    );
}

#[test]
fn decode_constructor_has_no_source() {
    let error = OpenAiSpecError::decode("decode failed");
    assert!(
        std::error::Error::source(&error).is_none(),
        "decode constructor should not attach a source"
    );
}
