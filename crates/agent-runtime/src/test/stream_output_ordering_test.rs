use agent_core::{CanonicalStreamEvent, ContentPart, ResponseFormat, StreamOutputItemEnd, StreamOutputItemStart};
use serde_json::json;

use super::stream_test_fixtures::*;

#[test]
fn reducer_prefers_tool_call_id_then_item_id_then_tool_call_index_when_matching() {
    let response = response_from_events(
        ResponseFormat::Text,
        vec![
            CanonicalStreamEvent::OutputItemStarted {
                output_index: 0,
                item: StreamOutputItemStart::ToolCall {
                    item_id: Some("item_a".to_string()),
                    tool_call_id: Some("call_a".to_string()),
                    name: "alpha".to_string(),
                },
            },
            CanonicalStreamEvent::OutputItemStarted {
                output_index: 0,
                item: StreamOutputItemStart::ToolCall {
                    item_id: Some("item_b".to_string()),
                    tool_call_id: Some("call_b".to_string()),
                    name: "beta".to_string(),
                },
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 0,
                tool_call_index: 99,
                item_id: Some("item_b".to_string()),
                tool_call_id: Some("call_a".to_string()),
                tool_name: None,
                delta: "{\"match\":\"tool_call_id\"}".to_string(),
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 0,
                tool_call_index: 77,
                item_id: Some("item_b".to_string()),
                tool_call_id: None,
                tool_name: None,
                delta: "{\"match\":\"item_id\"}".to_string(),
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 3,
                tool_call_index: 7,
                item_id: None,
                tool_call_id: None,
                tool_name: Some("gamma".to_string()),
                delta: "{\"match\":\"tool_call_index".to_string(),
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 3,
                tool_call_index: 7,
                item_id: None,
                tool_call_id: None,
                tool_name: None,
                delta: "\"}".to_string(),
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 0,
                item: StreamOutputItemEnd::ToolCall {
                    item_id: Some("item_a".to_string()),
                    tool_call_id: Some("call_a".to_string()),
                    name: "alpha".to_string(),
                    arguments_json_text: String::new(),
                },
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 0,
                item: StreamOutputItemEnd::ToolCall {
                    item_id: Some("item_b".to_string()),
                    tool_call_id: Some("call_b".to_string()),
                    name: "beta".to_string(),
                    arguments_json_text: String::new(),
                },
            },
        ],
        Vec::new(),
    )
    .expect("response should be built");

    assert_eq!(
        response.output.content,
        vec![
            ContentPart::tool_call("call_a", "alpha", json!({"match":"tool_call_id"})),
            ContentPart::tool_call("call_b", "beta", json!({"match":"item_id"})),
            ContentPart::tool_call(
                "stream_tool_call_2",
                "gamma",
                json!({"match":"tool_call_index"})
            ),
        ]
    );
}

#[test]
fn reducer_preserves_output_index_then_ordinal_order_for_mixed_parts() {
    let response = response_from_events(
        ResponseFormat::Text,
        vec![
            CanonicalStreamEvent::OutputItemStarted {
                output_index: 0,
                item: StreamOutputItemStart::ToolCall {
                    item_id: Some("tool_0".to_string()),
                    tool_call_id: Some("call_0".to_string()),
                    name: "first_tool".to_string(),
                },
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 0,
                tool_call_index: 0,
                item_id: Some("tool_0".to_string()),
                tool_call_id: Some("call_0".to_string()),
                tool_name: None,
                delta: "{\"order\":1}".to_string(),
            },
            CanonicalStreamEvent::OutputItemStarted {
                output_index: 0,
                item: StreamOutputItemStart::Message {
                    item_id: Some("msg_0".to_string()),
                    role: agent_core::MessageRole::Assistant,
                },
            },
            CanonicalStreamEvent::TextDelta {
                output_index: 0,
                content_index: 0,
                item_id: Some("msg_0".to_string()),
                delta: "second".to_string(),
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 0,
                item: StreamOutputItemEnd::ToolCall {
                    item_id: Some("tool_0".to_string()),
                    tool_call_id: Some("call_0".to_string()),
                    name: "first_tool".to_string(),
                    arguments_json_text: String::new(),
                },
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 0,
                item: StreamOutputItemEnd::Message {
                    item_id: Some("msg_0".to_string()),
                },
            },
            CanonicalStreamEvent::TextDelta {
                output_index: 1,
                content_index: 0,
                item_id: Some("msg_1".to_string()),
                delta: "third".to_string(),
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 1,
                item: StreamOutputItemEnd::Message {
                    item_id: Some("msg_1".to_string()),
                },
            },
        ],
        Vec::new(),
    )
    .expect("response should be built");

    assert_eq!(
        response.output.content,
        vec![
            ContentPart::tool_call("call_0", "first_tool", json!({"order":1})),
            ContentPart::text("second"),
            ContentPart::text("third"),
        ]
    );
}
