use serde_json::json;

use agent_core::types::{
    ContentPart, Message, MessageRole, ResponseFormat, ToolCall, ToolChoice, ToolDefinition,
    ToolResult, ToolResultContent,
};

use super::AnthropicFamilyErrorKind;
use super::anthropic_test_helpers::*;

#[test]
fn encode_basic_text_message() {
    let request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);

    let encoded = encode_anthropic_request(request.clone()).expect("encode should succeed");

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

    let encoded = encode_anthropic_request(request.clone()).expect("encode should succeed");
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
        encode_anthropic_request(request.clone())
            .expect("encode none")
            .body
            .pointer("/tool_choice/type"),
        Some(&json!("none"))
    );

    request.tool_choice = ToolChoice::Auto;
    assert_eq!(
        encode_anthropic_request(request.clone())
            .expect("encode auto")
            .body
            .pointer("/tool_choice/type"),
        Some(&json!("auto"))
    );

    request.tool_choice = ToolChoice::Required;
    assert_eq!(
        encode_anthropic_request(request.clone())
            .expect("encode required")
            .body
            .pointer("/tool_choice/type"),
        Some(&json!("any"))
    );

    request.tool_choice = ToolChoice::Specific {
        name: "calculator".to_string(),
    };
    let encoded = encode_anthropic_request(request.clone()).expect("encode specific");
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
    assert_eq!(
        encoded.body.pointer("/tools/0/description"),
        Some(&json!("compute expression"))
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
            "required": ["city"]
        }),
    }];

    let encoded = encode_anthropic_request(request).expect("encode should succeed");

    assert_eq!(
        encoded.body.pointer("/tools/0/name"),
        Some(&json!("lookup_weather"))
    );
    assert_eq!(
        encoded.body.pointer("/tools/0/description"),
        Some(&json!("Look up forecast details"))
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

    let encoded = encode_anthropic_request(request.clone()).expect("encode should succeed");
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

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
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

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
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
    let empty_name_error =
        encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(
        empty_name_error.kind(),
        AnthropicFamilyErrorKind::Validation
    );
    assert!(empty_name_error.message().contains("non-empty names"));

    request.tools = vec![ToolDefinition {
        name: "tool".to_string(),
        description: None,
        parameters_schema: json!("not-object"),
    }];
    let bad_schema_error =
        encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(
        bad_schema_error.kind(),
        AnthropicFamilyErrorKind::Validation
    );
    assert!(bad_schema_error.message().contains("must be a JSON object"));
}

#[test]
fn encode_rejects_temperature_out_of_range() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.temperature = Some(1.1);

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
    assert!(
        error
            .message()
            .contains("temperature must be in [0.0, 1.0]")
    );
}

#[test]
fn encode_rejects_top_p_out_of_range() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.top_p = Some(-0.1);

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
    assert!(error.message().contains("top_p must be in [0.0, 1.0]"));
}

#[test]
fn encode_rejects_zero_max_output_tokens() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.max_output_tokens = Some(0);

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
    assert!(
        error
            .message()
            .contains("max_output_tokens must be at least 1")
    );
}

#[test]
fn encode_rejects_empty_stop_sequence() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);
    request.stop = vec!["".to_string()];

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
    assert!(error.message().contains("must not contain empty strings"));
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

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::ProtocolViolation);
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

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
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

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
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

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
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

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::ProtocolViolation);
    assert!(
        error
            .message()
            .contains("duplicate assistant tool_call id 'call_1'")
    );
}

#[test]
fn encode_rejects_non_object_tool_call_arguments_json() {
    let request = base_request(vec![Message {
        role: MessageRole::Assistant,
        content: vec![ContentPart::ToolCall {
            tool_call: ToolCall {
                id: "call_1".to_string(),
                name: "calculator".to_string(),
                arguments_json: json!("2+2"),
            },
        }],
    }]);

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
    assert!(
        error
            .message()
            .contains("arguments_json must be a JSON object")
    );
}

#[test]
fn encode_rejects_non_text_tool_result_parts() {
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
                    tool_call_id: "call_1".to_string(),
                    content: ToolResultContent::Parts {
                        parts: vec![ContentPart::ToolCall {
                            tool_call: ToolCall {
                                id: "nested".to_string(),
                                name: "bad".to_string(),
                                arguments_json: json!({"x":1}),
                            },
                        }],
                    },
                    raw_provider_content: None,
                },
            }],
        },
    ]);

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
    assert!(error.message().contains("must contain only text parts"));
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

    let error = encode_anthropic_request(request.clone()).expect_err("encode should fail");
    assert_eq!(error.kind(), AnthropicFamilyErrorKind::Validation);
    assert!(error.message().contains("assistant-prefill"));
}

#[test]
fn encode_maps_json_schema_response_format_to_output_config() {
    let mut request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "return json".to_string(),
        }],
    }]);
    request.response_format = ResponseFormat::JsonSchema {
        name: "shape".to_string(),
        schema: json!({
            "type": "object",
            "properties": {
                "ok": {"type": "boolean"}
            },
            "required": ["ok"]
        }),
    };

    let encoded = encode_anthropic_request(request.clone()).expect("encode should succeed");
    assert_eq!(
        encoded.body.pointer("/output_config/format/type"),
        Some(&json!("json_schema"))
    );
    assert_eq!(
        encoded.body.pointer("/output_config/format/schema"),
        Some(&json!({
            "type": "object",
            "properties": {
                "ok": {"type": "boolean"}
            },
            "required": ["ok"]
        }))
    );
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

    let encoded = encode_anthropic_request(request.clone()).expect("encode should succeed");
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
    let encoded = encode_anthropic_request(request.clone()).expect("encode should succeed");

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

    let encoded = encode_anthropic_request(request.clone()).expect("encode should succeed");
    assert_eq!(encoded.body["metadata"], json!({"user_id":"user-1"}));
    assert!(
        encoded
            .warnings
            .iter()
            .any(|warning| warning.code == "anthropic.encode.dropped_unsupported_metadata_keys")
    );
}
