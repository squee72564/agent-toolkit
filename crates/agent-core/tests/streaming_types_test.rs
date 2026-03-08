use std::fs;
use std::path::PathBuf;

use agent_core::types::{FinishReason, MessageRole, ProviderId};

use agent_core::stream::{
    CanonicalStreamEvent, CanonicalStreamProjector, ProviderRawStreamEvent, RawStreamPayload,
    StreamOutputItemEnd, StreamOutputItemStart,
};
use serde_json::Value;

#[test]
fn openai_basic_chat_projects_text_usage_and_completion() {
    let envelopes = project_fixture(
        ProviderId::OpenAi,
        "openai/responses/streaming/basic_chat/gpt-5.1.json",
    );
    let canonical = flatten_canonical(&envelopes);

    assert!(matches!(
        canonical.first(),
        Some(CanonicalStreamEvent::ResponseStarted {
            model: Some(model),
            response_id: Some(_),
        }) if model == "gpt-5.1-2025-11-13"
    ));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::OutputItemStarted {
            output_index: 0,
            item: StreamOutputItemStart::Message {
                role: MessageRole::Assistant,
                ..
            },
        }
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::TextDelta { delta, .. } if delta == "Running"
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::UsageUpdated { usage }
            if usage.input_tokens == Some(50)
                && usage.output_tokens == Some(16)
                && usage.total_tokens == Some(66)
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::Stop,
        }
    )));
}

#[test]
fn openai_tool_call_projects_argument_deltas_and_completion() {
    let envelopes = project_fixture(
        ProviderId::OpenAi,
        "openai/responses/streaming/tool_call/gpt-5.1.json",
    );
    let canonical = flatten_canonical(&envelopes);

    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::OutputItemStarted {
            output_index: 0,
            item: StreamOutputItemStart::ToolCall { name, .. },
        } if name == "get_weather"
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::ToolCallArgumentsDelta { delta, .. } if delta == "San"
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::OutputItemCompleted {
            output_index: 0,
            item: StreamOutputItemEnd::ToolCall {
                name,
                arguments_json_text,
                ..
            },
        } if name == "get_weather" && arguments_json_text == "{\"city\":\"San Francisco\"}"
    )));
    assert!(matches!(
        canonical.last(),
        Some(CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::ToolCalls,
        })
    ));
}

#[test]
fn anthropic_tool_call_projects_tool_use_and_usage() {
    let envelopes = project_fixture(
        ProviderId::Anthropic,
        "anthropic/responses/streaming/tool_call/claude-sonnet-4-5-20250929.json",
    );
    let canonical = flatten_canonical(&envelopes);

    assert!(matches!(
        canonical.first(),
        Some(CanonicalStreamEvent::ResponseStarted {
            model: Some(model),
            response_id: Some(_),
        }) if model == "claude-sonnet-4-5-20250929"
    ));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::OutputItemStarted {
            item: StreamOutputItemStart::ToolCall { name, .. },
            ..
        } if name == "calculator"
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::ToolCallArgumentsDelta { delta, .. } if delta == "{\"expres"
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::OutputItemCompleted {
            item: StreamOutputItemEnd::ToolCall {
                name,
                arguments_json_text,
                ..
            },
            ..
        } if name == "calculator"
            && arguments_json_text == "{\"expression\": \"2 * 2 + sqrt(2)\"}"
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::UsageUpdated { usage }
            if usage.input_tokens == Some(591)
                && usage.output_tokens == Some(60)
                && usage.cached_input_tokens == Some(0)
    )));
    assert!(matches!(
        canonical.last(),
        Some(CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::ToolCalls,
        })
    ));
}

#[test]
fn openrouter_basic_chat_ignores_processing_comments_and_synthesizes_message_flow() {
    let envelopes = project_fixture(
        ProviderId::OpenRouter,
        "openrouter/responses/streaming/basic_chat/anthropic.claude-sonnet-4.5.json",
    );

    assert!(matches!(
        envelopes[0].raw.payload,
        RawStreamPayload::Comment { .. }
    ));
    assert!(envelopes[0].canonical.is_empty());

    let canonical = flatten_canonical(&envelopes);
    assert!(matches!(
        canonical.first(),
        Some(CanonicalStreamEvent::ResponseStarted {
            model: Some(model),
            response_id: Some(_),
        }) if model == "anthropic/claude-4.5-sonnet-20250929"
    ));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::OutputItemStarted {
            item: StreamOutputItemStart::Message {
                role: MessageRole::Assistant,
                ..
            },
            ..
        }
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::TextDelta { delta, .. } if delta == "I'm doing well, thank you!"
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::UsageUpdated { usage }
            if usage.input_tokens == Some(44)
                && usage.output_tokens == Some(11)
                && usage.total_tokens == Some(55)
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::Stop,
        }
    )));
}

#[test]
fn openrouter_tool_call_reasoning_emits_no_canonical_reasoning_events() {
    let envelopes = project_fixture(
        ProviderId::OpenRouter,
        "openrouter/responses/streaming/tool_call_reasoning/openai.gpt-5.1.json",
    );
    let canonical = flatten_canonical(&envelopes);

    assert!(matches!(
        envelopes[0].raw.payload,
        RawStreamPayload::Comment { .. }
    ));
    assert!(envelopes[0].canonical.is_empty());
    assert!(matches!(
        envelopes[1].raw.payload,
        RawStreamPayload::Json { .. }
    ));
    assert!(matches!(
        canonical.first(),
        Some(CanonicalStreamEvent::ResponseStarted { .. })
    ));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::OutputItemStarted {
            item: StreamOutputItemStart::Message { .. },
            ..
        }
    )));
    assert!(canonical.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::TextDelta { delta, .. } if delta == "The"
    )));
    assert!(matches!(
        canonical.last(),
        Some(CanonicalStreamEvent::UsageUpdated { .. })
    ));
}

fn project_fixture(
    provider: ProviderId,
    relative_path: &str,
) -> Vec<agent_core::stream::CanonicalStreamEnvelope> {
    let fixture = read_fixture(relative_path);
    let events = fixture["stream"]["events"]
        .as_array()
        .expect("fixture stream events");
    let mut projector = CanonicalStreamProjector::default();

    events
        .iter()
        .map(|event| projector.project(raw_event_from_fixture(provider, event)))
        .collect()
}

fn raw_event_from_fixture(provider: ProviderId, event: &Value) -> ProviderRawStreamEvent {
    let sequence = event["index"].as_u64().expect("fixture event index");
    let raw = event["raw"].as_str().unwrap_or_default();
    let data = event["data"].as_str().unwrap_or_default();

    if raw.starts_with(':') && data.is_empty() {
        return ProviderRawStreamEvent::from_sse_comment(
            provider,
            sequence,
            raw.trim_start_matches(':').trim(),
        );
    }

    ProviderRawStreamEvent::from_sse(
        provider,
        sequence,
        event["event"].as_str().map(ToOwned::to_owned),
        None,
        None,
        data.to_string(),
    )
}

fn flatten_canonical(
    envelopes: &[agent_core::stream::CanonicalStreamEnvelope],
) -> Vec<CanonicalStreamEvent> {
    envelopes
        .iter()
        .flat_map(|envelope| envelope.canonical.iter().cloned())
        .collect()
}

fn read_fixture(relative_path: &str) -> Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../agent-providers/data");
    path.push(relative_path);

    serde_json::from_str(&fs::read_to_string(path).expect("read fixture")).expect("parse fixture")
}
