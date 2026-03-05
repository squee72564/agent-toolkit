use agent_core::types::{ContentPart, Message, MessageRole, ToolResultContent};
use serde_json::json;

#[test]
fn message_user_text_constructor_sets_role_and_single_text_part() {
    let message = Message::user_text("hello");
    assert_eq!(message.role, MessageRole::User);
    assert_eq!(
        message.content,
        vec![ContentPart::Text {
            text: "hello".to_string()
        }]
    );
}

#[test]
fn message_system_and_assistant_text_constructors_work() {
    let system = Message::system_text("system rules");
    let assistant = Message::assistant_text("assistant reply");

    assert_eq!(system.role, MessageRole::System);
    assert_eq!(
        system.content,
        vec![ContentPart::Text {
            text: "system rules".to_string()
        }]
    );

    assert_eq!(assistant.role, MessageRole::Assistant);
    assert_eq!(
        assistant.content,
        vec![ContentPart::Text {
            text: "assistant reply".to_string()
        }]
    );
}

#[test]
fn content_part_tool_call_constructor_builds_expected_shape() {
    let part = ContentPart::tool_call("call_1", "search", json!({ "query": "rust" }));

    assert_eq!(
        part,
        ContentPart::ToolCall {
            tool_call: agent_core::types::ToolCall {
                id: "call_1".to_string(),
                name: "search".to_string(),
                arguments_json: json!({ "query": "rust" }),
            }
        }
    );
}

#[test]
fn message_assistant_tool_call_constructor_builds_expected_shape() {
    let message = Message::assistant_tool_call("call_1", "search", json!({ "query": "rust" }));

    assert_eq!(message.role, MessageRole::Assistant);
    assert_eq!(
        message.content,
        vec![ContentPart::ToolCall {
            tool_call: agent_core::types::ToolCall {
                id: "call_1".to_string(),
                name: "search".to_string(),
                arguments_json: json!({ "query": "rust" }),
            }
        }]
    );
}

#[test]
fn content_part_tool_result_json_and_text_default_raw_none() {
    let json_part = ContentPart::tool_result_json("call_1", json!({ "ok": true }));
    let text_part = ContentPart::tool_result_text("call_2", "done");

    assert!(matches!(
        json_part,
        ContentPart::ToolResult { ref tool_result }
            if tool_result.tool_call_id == "call_1"
                && tool_result.content == ToolResultContent::Json {
                    value: json!({ "ok": true })
                }
                && tool_result.raw_provider_content.is_none()
    ));

    assert!(matches!(
        text_part,
        ContentPart::ToolResult { ref tool_result }
            if tool_result.tool_call_id == "call_2"
                && tool_result.content == ToolResultContent::Text {
                    text: "done".to_string()
                }
                && tool_result.raw_provider_content.is_none()
    ));
}

#[test]
fn tool_result_with_raw_variants_populate_raw_provider_content() {
    let raw = json!({ "provider": "openai", "payload": { "x": 1 } });
    let json_part =
        ContentPart::tool_result_json_with_raw("call_1", json!({ "ok": true }), raw.clone());
    let text_part = ContentPart::tool_result_text_with_raw("call_2", "done", raw.clone());

    assert!(matches!(
        json_part,
        ContentPart::ToolResult { ref tool_result }
            if tool_result.raw_provider_content == Some(raw.clone())
    ));
    assert!(matches!(
        text_part,
        ContentPart::ToolResult { ref tool_result }
            if tool_result.raw_provider_content == Some(raw)
    ));
}

#[test]
fn message_tool_result_helpers_create_tool_role_messages() {
    let json_message = Message::tool_result_json("call_1", json!({ "temp_f": 72 }));
    let text_message = Message::tool_result_text("call_2", "sunny");

    assert_eq!(json_message.role, MessageRole::Tool);
    assert_eq!(text_message.role, MessageRole::Tool);
    assert_eq!(json_message.content.len(), 1);
    assert_eq!(text_message.content.len(), 1);
}

#[test]
fn message_tool_result_helpers_with_raw_create_tool_role_messages_and_preserve_raw() {
    let raw = json!({ "provider": "openai", "payload": { "x": 1 } });
    let json_message =
        Message::tool_result_json_with_raw("call_1", json!({ "temp_f": 72 }), raw.clone());
    let text_message = Message::tool_result_text_with_raw("call_2", "sunny", raw.clone());

    assert_eq!(json_message.role, MessageRole::Tool);
    assert_eq!(text_message.role, MessageRole::Tool);
    assert_eq!(json_message.content.len(), 1);
    assert_eq!(text_message.content.len(), 1);
    assert!(matches!(
        json_message.content[0],
        ContentPart::ToolResult { ref tool_result }
            if tool_result.tool_call_id == "call_1"
                && tool_result.content == ToolResultContent::Json {
                    value: json!({ "temp_f": 72 })
                }
                && tool_result.raw_provider_content == Some(raw.clone())
    ));
    assert!(matches!(
        text_message.content[0],
        ContentPart::ToolResult { ref tool_result }
            if tool_result.tool_call_id == "call_2"
                && tool_result.content == ToolResultContent::Text {
                    text: "sunny".to_string()
                }
                && tool_result.raw_provider_content == Some(raw)
    ));
}

#[test]
fn message_new_supports_multi_part_content() {
    let message = Message::new(
        MessageRole::Assistant,
        vec![
            ContentPart::text("first"),
            ContentPart::tool_call("call_1", "search", json!({ "query": "rust" })),
        ],
    );

    assert_eq!(message.role, MessageRole::Assistant);
    assert_eq!(message.content.len(), 2);
    assert_eq!(
        message.content[0],
        ContentPart::Text {
            text: "first".to_string()
        }
    );
    assert!(matches!(message.content[1], ContentPart::ToolCall { .. }));
}

#[test]
fn serde_roundtrip_of_helper_built_messages_matches_existing_contract() {
    let message = Message::assistant_tool_call("call_1", "search", json!({ "query": "rust" }));
    let serialized = serde_json::to_string(&message).expect("serialize message");
    let decoded: Message = serde_json::from_str(&serialized).expect("deserialize message");

    assert_eq!(decoded, message);
}

#[test]
fn serde_helper_message_omits_raw_provider_content_when_none() {
    let message = Message::tool_result_text("call_2", "done");
    let serialized = serde_json::to_value(&message).expect("serialize message");

    assert_eq!(serialized["role"]["type"], json!("tool"));
    assert_eq!(serialized["content"][0]["type"], json!("tool_result"));
    assert_eq!(
        serialized["content"][0]["tool_result"]["tool_call_id"],
        json!("call_2")
    );
    assert_eq!(
        serialized["content"][0]["tool_result"]["content"]["type"],
        json!("text")
    );
    assert_eq!(
        serialized["content"][0]["tool_result"]["content"]["text"],
        json!("done")
    );
    assert!(serialized["content"][0]["tool_result"]["raw_provider_content"].is_null());
}

#[test]
fn serde_helper_message_includes_raw_provider_content_when_present() {
    let raw = json!({ "provider": "openai", "payload": { "x": 1 } });
    let message = Message::tool_result_json_with_raw("call_1", json!({ "ok": true }), raw.clone());
    let serialized = serde_json::to_value(&message).expect("serialize message");

    assert_eq!(serialized["role"]["type"], json!("tool"));
    assert_eq!(serialized["content"][0]["type"], json!("tool_result"));
    assert_eq!(
        serialized["content"][0]["tool_result"]["tool_call_id"],
        json!("call_1")
    );
    assert_eq!(
        serialized["content"][0]["tool_result"]["content"]["type"],
        json!("json")
    );
    assert_eq!(
        serialized["content"][0]["tool_result"]["content"]["value"],
        json!({ "ok": true })
    );
    assert_eq!(
        serialized["content"][0]["tool_result"]["raw_provider_content"],
        raw
    );
}
