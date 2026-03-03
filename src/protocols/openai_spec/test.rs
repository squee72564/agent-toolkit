use std::collections::BTreeMap;

use serde_json::json;

use crate::core::types::{
    ContentPart, Message, MessageRole, Request, ResponseFormat, ToolCall, ToolChoice, ToolResult,
    ToolResultContent,
};

use super::OpenAiSpecError;
use super::encode::encode_openai_request;
use super::schema_rules::is_strict_compatible_schema;

fn base_request(messages: Vec<Message>) -> Request {
    Request {
        model_id: "gpt-4.1-mini".to_string(),
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

    let encoded = encode_openai_request(&request).expect("encoding should succeed");

    assert_eq!(encoded.body["model"], json!("gpt-4.1-mini"));
    assert_eq!(encoded.body["text"]["format"]["type"], json!("text"));
    assert_eq!(encoded.body["input"].as_array().map(Vec::len), Some(1));
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

    let error = encode_openai_request(&request).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::Validation { message } => {
            assert!(message.contains("requires at least one tool definition"));
        }
        _ => panic!("unexpected error variant"),
    }
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

    let error = encode_openai_request(&request).expect_err("encoding should fail");

    match error {
        OpenAiSpecError::ProtocolViolation { message } => {
            assert!(message.contains("tool_result_without_matching_tool_call"));
        }
        _ => panic!("unexpected error variant"),
    }
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

    let encoded = encode_openai_request(&request).expect("encoding should succeed");
    let input = encoded.body["input"]
        .as_array()
        .expect("input must be array");

    assert_eq!(input[0]["type"], json!("function_call"));
    assert_eq!(input[1]["type"], json!("function_call_output"));
}
