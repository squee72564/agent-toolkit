use agent_core::{
    CanonicalStreamEvent, FinishReason, MessageRole, ProviderKind, ProviderRawStreamEvent,
    StreamOutputItemEnd, StreamOutputItemStart,
};
use serde_json::Value;

use crate::platform::openrouter::stream::OpenRouterStreamProjector;
use crate::platform::test_fixtures::load_streaming_success_fixture;
use crate::streaming::ProviderStreamProjector;

#[test]
fn openrouter_stream_projector_completes_on_done_payload() {
    let mut projector = OpenRouterStreamProjector::default();

    let events = projector
        .project(ProviderRawStreamEvent::from_sse(
            ProviderKind::OpenRouter,
            1,
            None,
            None,
            None,
            "[DONE]",
        ))
        .expect("projection should succeed");

    assert_eq!(
        events,
        vec![CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::Other,
        }]
    );
}

#[test]
fn openrouter_stream_projector_ignores_leading_comments_and_completes_basic_chat_once() {
    let fixture = load_streaming_success_fixture("openrouter", "basic_chat", "openai.gpt-5.4");
    let mut projector = OpenRouterStreamProjector::default();
    let events = fixture_events(&fixture);

    for (sequence, event) in events.iter().take(2).enumerate() {
        let canonical = projector
            .project(ProviderRawStreamEvent::from_sse(
                ProviderKind::OpenRouter,
                u64::try_from(sequence + 1).expect("sequence fits in u64"),
                event.0.clone(),
                None,
                None,
                event.1.clone(),
            ))
            .expect("projection should succeed");
        assert!(
            canonical.is_empty(),
            "leading comment/empty events should not emit canonical events"
        );
    }

    let mut canonical = Vec::new();
    for (sequence, event) in events.into_iter().enumerate().skip(2) {
        canonical.extend(
            projector
                .project(ProviderRawStreamEvent::from_sse(
                    ProviderKind::OpenRouter,
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
            model: Some("openai/gpt-5.4-20260305".to_string()),
            response_id: Some("gen-1773017858-XXCcNEyekwclyaLS5JuS".to_string()),
        })
    );
    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemStarted {
            output_index: 0,
            item: StreamOutputItemStart::Message {
                item_id: Some("msg_tmp_w6z9h2jc6ki".to_string()),
                role: MessageRole::Assistant,
            },
        })
    );
    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemCompleted {
            output_index: 0,
            item: StreamOutputItemEnd::Message {
                item_id: Some("msg_tmp_w6z9h2jc6ki".to_string()),
            },
        })
    );
    assert!(canonical.contains(&CanonicalStreamEvent::UsageUpdated {
        usage: agent_core::Usage {
            input_tokens: Some(50),
            output_tokens: Some(11),
            cached_input_tokens: Some(0),
            total_tokens: Some(61),
        },
    }));

    let text = canonical
        .iter()
        .filter_map(|event| match event {
            CanonicalStreamEvent::TextDelta { delta, .. } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(text, "Doing well, ready to help!");

    let completed = canonical
        .iter()
        .filter(|event| {
            matches!(
                event,
                CanonicalStreamEvent::Completed {
                    finish_reason: FinishReason::Stop
                }
            )
        })
        .count();
    assert_eq!(completed, 1, "expected a single stop completion event");
}

#[test]
fn openrouter_stream_projector_accumulates_tool_call_fixture_arguments() {
    let fixture = load_streaming_success_fixture("openrouter", "tool_call", "openai.gpt-5.4");
    let mut projector = OpenRouterStreamProjector::default();
    let mut canonical = Vec::new();

    for (sequence, event) in fixture_events(&fixture).into_iter().enumerate() {
        canonical.extend(
            projector
                .project(ProviderRawStreamEvent::from_sse(
                    ProviderKind::OpenRouter,
                    u64::try_from(sequence + 1).expect("sequence fits in u64"),
                    event.0,
                    None,
                    None,
                    event.1,
                ))
                .expect("projection should succeed"),
        );
    }

    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemStarted {
            output_index: 0,
            item: StreamOutputItemStart::ToolCall {
                item_id: Some("fc_tmp_1trdxhy16ld".to_string()),
                tool_call_id: Some("call_LUFIV7qTRIhlKVibajduem4d".to_string()),
                name: "get_weather".to_string(),
            },
        })
    );
    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemCompleted {
            output_index: 0,
            item: StreamOutputItemEnd::ToolCall {
                item_id: Some("fc_tmp_1trdxhy16ld".to_string()),
                tool_call_id: Some("call_LUFIV7qTRIhlKVibajduem4d".to_string()),
                name: "get_weather".to_string(),
                arguments_json_text: "{\"city\":\"San Francisco\"}".to_string(),
            },
        })
    );

    let tool_arguments = canonical
        .iter()
        .filter_map(|event| match event {
            CanonicalStreamEvent::ToolCallArgumentsDelta { delta, .. } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(tool_arguments, "{\"city\":\"San Francisco\"}");

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
    assert_eq!(completed, 1, "expected a single tool-call completion event");
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
