use agent_core::{CanonicalStreamEvent, ResponseFormat, RuntimeWarning, StreamOutputItemStart};
use serde_json::json;

use super::stream_test_fixtures::*;

#[test]
fn structured_output_valid_json_object_populates_output_without_warning() {
    let response = response_from_events(
        ResponseFormat::JsonObject,
        vec![CanonicalStreamEvent::TextDelta {
            output_index: 0,
            content_index: 0,
            item_id: Some("msg".to_string()),
            delta: "{\"ok\":true}".to_string(),
        }],
        Vec::new(),
    )
    .expect("response should be built");

    assert_eq!(response.output.structured_output, Some(json!({"ok": true})));
    assert!(response.warnings.is_empty());
}

#[test]
fn structured_output_non_object_json_warns() {
    let response = response_from_events(
        ResponseFormat::JsonObject,
        vec![CanonicalStreamEvent::TextDelta {
            output_index: 0,
            content_index: 0,
            item_id: Some("msg".to_string()),
            delta: "[1,2,3]".to_string(),
        }],
        Vec::new(),
    )
    .expect("response should be built");

    assert_eq!(response.output.structured_output, None);
    assert_eq!(
        response.warnings,
        vec![RuntimeWarning {
            code: "runtime.stream.structured_output_not_object".to_string(),
            message: "streamed structured output was not a JSON object".to_string(),
        }]
    );
}

#[test]
fn structured_output_invalid_json_warns() {
    let response = response_from_events(
        ResponseFormat::JsonObject,
        vec![CanonicalStreamEvent::TextDelta {
            output_index: 0,
            content_index: 0,
            item_id: Some("msg".to_string()),
            delta: "{oops".to_string(),
        }],
        Vec::new(),
    )
    .expect("response should be built");

    assert_eq!(response.output.structured_output, None);
    assert_eq!(response.warnings.len(), 1);
    assert_eq!(
        response.warnings[0].code,
        "runtime.stream.structured_output_parse_failed"
    );
    assert!(
        response.warnings[0]
            .message
            .contains("failed to parse streamed structured output")
    );
}

#[test]
fn structured_output_without_text_part_has_no_warning() {
    let response = response_from_events(
        ResponseFormat::JsonObject,
        vec![CanonicalStreamEvent::OutputItemStarted {
            output_index: 0,
            item: StreamOutputItemStart::ToolCall {
                item_id: Some("item_1".to_string()),
                tool_call_id: Some("call_1".to_string()),
                name: "lookup".to_string(),
            },
        }],
        Vec::new(),
    )
    .expect("response should be built");

    assert_eq!(response.output.structured_output, None);
    assert!(response.warnings.is_empty());
}
