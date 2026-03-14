use agent_core::{
    CanonicalStreamEvent, FinishReason, MessageRole, ProviderKind, ProviderRawStreamEvent,
    StreamOutputItemEnd, StreamOutputItemStart,
};
use serde_json::Value;

use super::anthropic_stream_projector::AnthropicStreamProjector;
use crate::streaming::ProviderStreamProjector;
use crate::test_fixtures::load_streaming_success_fixture;

#[test]
fn anthropic_stream_projector_tracks_message_lifecycle() {
    let mut projector = AnthropicStreamProjector::default();

    let started = projector
        .project(ProviderRawStreamEvent::from_sse(
            ProviderKind::Anthropic,
            1,
            Some("message_start".to_string()),
            None,
            None,
            r#"{"message":{"id":"msg_1","model":"claude-sonnet-4-6"}}"#,
        ))
        .expect("projection should succeed");
    let completed = projector
        .project(ProviderRawStreamEvent::from_sse(
            ProviderKind::Anthropic,
            2,
            Some("message_stop".to_string()),
            None,
            None,
            r#"{}"#,
        ))
        .expect("projection should succeed");

    assert_eq!(
        started,
        vec![CanonicalStreamEvent::ResponseStarted {
            model: Some("claude-sonnet-4-6".to_string()),
            response_id: Some("msg_1".to_string()),
        }]
    );
    assert_eq!(
        completed,
        vec![CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::Other,
        }]
    );
}

#[test]
fn anthropic_stream_projector_preserves_incremental_stop_reason_and_tool_call_fixture_semantics() {
    let fixture = load_streaming_success_fixture("anthropic", "tool_call", "claude-sonnet-4-6");
    let mut projector = AnthropicStreamProjector::default();
    let mut canonical = Vec::new();

    for (sequence, event) in fixture_events(&fixture).into_iter().enumerate() {
        canonical.extend(
            projector
                .project(ProviderRawStreamEvent::from_sse(
                    ProviderKind::Anthropic,
                    u64::try_from(sequence + 1).expect("sequence fits in u64"),
                    event.0,
                    None,
                    None,
                    event.1,
                ))
                .expect("projection should succeed"),
        );
    }

    assert_eq!(
        canonical.first(),
        Some(&CanonicalStreamEvent::ResponseStarted {
            model: Some("claude-sonnet-4-6".to_string()),
            response_id: Some("msg_01MHAhj8GLvdCak6Q86FeZqx".to_string()),
        })
    );
    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemStarted {
            output_index: 0,
            item: StreamOutputItemStart::Message {
                item_id: None,
                role: MessageRole::Assistant,
            },
        })
    );
    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemCompleted {
            output_index: 0,
            item: StreamOutputItemEnd::Message { item_id: None },
        })
    );
    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemStarted {
            output_index: 1,
            item: StreamOutputItemStart::ToolCall {
                item_id: None,
                tool_call_id: Some("toolu_01Pc83s2bRmUoEULzBSG57b2".to_string()),
                name: "calculator".to_string(),
            },
        })
    );
    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemCompleted {
            output_index: 1,
            item: StreamOutputItemEnd::ToolCall {
                item_id: None,
                tool_call_id: Some("toolu_01Pc83s2bRmUoEULzBSG57b2".to_string()),
                name: "calculator".to_string(),
                arguments_json_text: "{\"expression\": \"2 * 2 + sqrt(2)\"}".to_string(),
            },
        })
    );
    assert!(canonical.contains(&CanonicalStreamEvent::UsageUpdated {
        usage: agent_core::Usage {
            input_tokens: Some(592),
            output_tokens: Some(70),
            cached_input_tokens: Some(0),
            total_tokens: None,
        },
    }));

    let text = canonical
        .iter()
        .filter_map(|event| match event {
            CanonicalStreamEvent::TextDelta { delta, .. } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(text, "Sure! Let me calculate that for you!");

    let tool_arguments = canonical
        .iter()
        .filter_map(|event| match event {
            CanonicalStreamEvent::ToolCallArgumentsDelta { delta, .. } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(tool_arguments, "{\"expression\": \"2 * 2 + sqrt(2)\"}");

    let completed = canonical
        .iter()
        .filter(|event| {
            matches!(
                event,
                CanonicalStreamEvent::Completed {
                    finish_reason: FinishReason::ToolCalls
                }
            )
        })
        .count();
    assert_eq!(completed, 1, "expected one terminal completion event");
}

#[test]
fn anthropic_stream_projector_maps_basic_chat_fixture_to_stop_completion() {
    let fixture = load_streaming_success_fixture("anthropic", "basic_chat", "claude-sonnet-4-6");
    let mut projector = AnthropicStreamProjector::default();
    let mut canonical = Vec::new();

    for (sequence, event) in fixture_events(&fixture).into_iter().enumerate() {
        canonical.extend(
            projector
                .project(ProviderRawStreamEvent::from_sse(
                    ProviderKind::Anthropic,
                    u64::try_from(sequence + 1).expect("sequence fits in u64"),
                    event.0,
                    None,
                    None,
                    event.1,
                ))
                .expect("projection should succeed"),
        );
    }

    let text = canonical
        .iter()
        .filter_map(|event| match event {
            CanonicalStreamEvent::TextDelta { delta, .. } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(text, "Ready, helpful, and eager today! \u{1F60A}");
    assert_eq!(
        canonical.last(),
        Some(&CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::Stop,
        })
    );
}

fn fixture_events(fixture: &Value) -> Vec<(Option<String>, String)> {
    fixture
        .get("stream")
        .and_then(|stream| stream.get("events"))
        .and_then(Value::as_array)
        .expect("fixture should contain stream.events")
        .iter()
        .map(|event| {
            (
                event
                    .get("event")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                event
                    .get("data")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}
