use agent_core::{
    CanonicalStreamEvent, FinishReason, ProviderKind, ProviderRawStreamEvent, StreamOutputItemEnd,
    StreamOutputItemStart,
};
use serde_json::Value;

use crate::family_codec::openai_compatible_stream_projector::OpenAiStreamProjector;
use crate::fixture_tests::load_streaming_success_fixture;
use crate::stream_projector::ProviderStreamProjector;

#[test]
fn openai_stream_projector_emits_started_and_completed_events() {
    let mut projector = OpenAiStreamProjector::default();

    let started = projector
        .project(ProviderRawStreamEvent::from_sse(
            ProviderKind::OpenAi,
            1,
            Some("response.created".to_string()),
            None,
            None,
            r#"{"type":"response.created","response":{"id":"resp_1","model":"gpt-5-mini"}}"#,
        ))
        .expect("projection should succeed");
    let completed = projector
        .project(ProviderRawStreamEvent::from_sse(
            ProviderKind::OpenAi,
            2,
            Some("response.completed".to_string()),
            None,
            None,
            r#"{"type":"response.completed","response":{"output":[],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}}"#,
        ))
        .expect("projection should succeed");

    assert_eq!(
        started,
        vec![CanonicalStreamEvent::ResponseStarted {
            model: Some("gpt-5-mini".to_string()),
            response_id: Some("resp_1".to_string()),
        }]
    );
    assert!(completed.contains(&CanonicalStreamEvent::Completed {
        finish_reason: FinishReason::Stop,
    }));
}

#[test]
fn openai_stream_projector_accumulates_tool_call_fixture_arguments() {
    let fixture = load_streaming_success_fixture("openai", "tool_call", "gpt-5-mini");
    let mut projector = OpenAiStreamProjector::default();
    let mut canonical = Vec::new();

    for (sequence, event) in fixture_events(&fixture).into_iter().enumerate() {
        canonical.extend(
            projector
                .project(ProviderRawStreamEvent::from_sse(
                    ProviderKind::OpenAi,
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
            output_index: 1,
            item: StreamOutputItemStart::ToolCall {
                item_id: Some("fc_0c39f51aabc7b6d00169add64a2d388195874b47205015db51".to_string()),
                tool_call_id: Some("call_gJs6O3wJEWPZLcQwjUDop4Rz".to_string()),
                name: "get_weather".to_string(),
            },
        })
    );
    assert!(
        canonical.contains(&CanonicalStreamEvent::OutputItemCompleted {
            output_index: 1,
            item: StreamOutputItemEnd::ToolCall {
                item_id: Some("fc_0c39f51aabc7b6d00169add64a2d388195874b47205015db51".to_string()),
                tool_call_id: Some("call_gJs6O3wJEWPZLcQwjUDop4Rz".to_string()),
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

    assert!(canonical.contains(&CanonicalStreamEvent::Completed {
        finish_reason: FinishReason::ToolCalls,
    }));
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
