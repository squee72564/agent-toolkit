use serde_json::json;

use agent_core::types::{
    ContentPart, Message, MessageRole, ResponseFormat, ToolCall, ToolChoice, ToolDefinition,
    ToolResult, ToolResultContent,
};

use super::OpenAiFamilyError;
use super::openai_test_helpers::*;
use super::schema_rules::{canonicalize_json, is_strict_compatible_schema, stable_json_string};

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
            OpenAiFamilyError::Validation { message }
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
            OpenAiFamilyError::ProtocolViolation { message }
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
    let request = base_request(vec![Message::user_text("hello")]);

    let error =
        encode_openai_request_with_model(request.clone(), "   ").expect_err("encoding should fail");

    match error {
        OpenAiFamilyError::Validation { message } => {
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
        OpenAiFamilyError::Validation { message } => {
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
        OpenAiFamilyError::Validation { message } => {
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
        OpenAiFamilyError::Validation { message } => {
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
        OpenAiFamilyError::Validation { message } => {
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
        OpenAiFamilyError::Validation { message } => {
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
        OpenAiFamilyError::ProtocolViolation { message } => {
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
        OpenAiFamilyError::Validation { message } => {
            assert!(message.contains("tool_result tool_call_id must not be empty"));
        }
        _ => panic!("unexpected error variant"),
    }
}
