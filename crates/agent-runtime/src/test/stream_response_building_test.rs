use std::collections::BTreeMap;

use agent_core::{
    CanonicalStreamEvent, ContentPart, FinishReason, Message, ProviderKind, ResponseFormat,
    StreamOutputItemEnd, StreamOutputItemStart, TaskRequest, ToolChoice,
};
use agent_providers::error::AdapterErrorKind;
use serde_json::json;

use crate::planner;
use crate::provider_client::ProviderClient;
use crate::provider_runtime::ProviderAttemptOutcome;
use crate::{MessageCreateInput, RuntimeErrorKind};

use super::stream_test_fixtures::*;

#[tokio::test]
async fn current_non_streaming_api_rejects_stream_requests() {
    let client = super::test_provider_client(ProviderKind::OpenAi);

    let error = client
        .messages()
        .execute(
            MessageCreateInput::user("hello")
                .into_task_request()
                .expect("task request should build"),
            crate::ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..crate::ExecutionOptions::default()
            },
        )
        .await
        .expect_err("streaming execution should be rejected on the current response API");

    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
    assert!(
        error.message.contains("ResponseMode::NonStreaming"),
        "unexpected message: {}",
        error.message
    );
}

#[tokio::test]
async fn runtime_executes_openai_sse_plan_and_builds_response() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello from stream\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let runtime = test_provider_runtime(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let attempt = runtime
        .execute_attempt(
            planner::plan_direct_attempt(
                &ProviderClient::new(runtime.clone()),
                &TaskRequest {
                    messages: vec![Message::user_text("hello")],
                    tools: Vec::new(),
                    tool_choice: ToolChoice::Auto,
                    response_format: ResponseFormat::Text,
                    temperature: None,
                    top_p: None,
                    max_output_tokens: None,
                    stop: Vec::new(),
                    metadata: BTreeMap::new(),
                },
                &crate::AttemptSpec::to(crate::Target::new(runtime.instance_id.clone())),
                &crate::ExecutionOptions {
                    response_mode: crate::ResponseMode::Streaming,
                    ..crate::ExecutionOptions::default()
                },
            )
            .expect("planning should succeed"),
        )
        .await;

    match attempt {
        ProviderAttemptOutcome::Success {
            response,
            status_code,
            request_id,
            ..
        } => {
            assert_eq!(status_code, Some(200));
            assert_eq!(request_id.as_deref(), Some("req_sse"));
            assert_eq!(response.model, "gpt-5-mini");
            assert_eq!(
                response.output.content,
                vec![agent_core::ContentPart::text("hello from stream")]
            );
            assert_eq!(response.usage.input_tokens, Some(1));
            assert_eq!(response.usage.output_tokens, Some(2));
            assert_eq!(response.usage.total_tokens, Some(3));
            assert!(response.raw_provider_response.is_some());
        }
        ProviderAttemptOutcome::Failure { error, .. } => {
            panic!("expected SSE attempt success, got error: {error}")
        }
    }
}

#[test]
fn reducer_reconstructs_text_started_by_explicit_start_and_implicit_delta() {
    let response = response_from_events(
        ResponseFormat::Text,
        vec![
            CanonicalStreamEvent::ResponseStarted {
                model: Some("gpt-5-mini".to_string()),
                response_id: Some("resp_1".to_string()),
            },
            CanonicalStreamEvent::OutputItemStarted {
                output_index: 0,
                item: StreamOutputItemStart::Message {
                    item_id: Some("msg_explicit".to_string()),
                    role: agent_core::MessageRole::Assistant,
                },
            },
            CanonicalStreamEvent::TextDelta {
                output_index: 0,
                content_index: 0,
                item_id: Some("msg_explicit".to_string()),
                delta: "hello".to_string(),
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 0,
                item: StreamOutputItemEnd::Message {
                    item_id: Some("msg_explicit".to_string()),
                },
            },
            CanonicalStreamEvent::TextDelta {
                output_index: 1,
                content_index: 0,
                item_id: Some("msg_implicit".to_string()),
                delta: " world".to_string(),
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 1,
                item: StreamOutputItemEnd::Message {
                    item_id: Some("msg_implicit".to_string()),
                },
            },
            CanonicalStreamEvent::Completed {
                finish_reason: FinishReason::Stop,
            },
        ],
        Vec::new(),
    )
    .expect("response should be built");

    assert_eq!(
        response.output.content,
        vec![ContentPart::text("hello"), ContentPart::text(" world")]
    );
}

#[test]
fn reducer_reconstructs_tool_call_from_start_deltas_and_completion() {
    let response = response_from_events(
        ResponseFormat::Text,
        vec![
            CanonicalStreamEvent::OutputItemStarted {
                output_index: 0,
                item: StreamOutputItemStart::ToolCall {
                    item_id: Some("item_1".to_string()),
                    tool_call_id: Some("call_1".to_string()),
                    name: "lookup".to_string(),
                },
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 0,
                tool_call_index: 3,
                item_id: Some("item_1".to_string()),
                tool_call_id: None,
                tool_name: None,
                delta: "{\"city\":\"San".to_string(),
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 0,
                tool_call_index: 3,
                item_id: Some("item_1".to_string()),
                tool_call_id: None,
                tool_name: None,
                delta: " Francisco\"}".to_string(),
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 0,
                item: StreamOutputItemEnd::ToolCall {
                    item_id: Some("item_1".to_string()),
                    tool_call_id: Some("call_1".to_string()),
                    name: "lookup".to_string(),
                    arguments_json_text: String::new(),
                },
            },
        ],
        vec![CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::ToolCalls,
        }],
    )
    .expect("response should be built");

    assert_eq!(
        response.output.content,
        vec![ContentPart::tool_call(
            "call_1",
            "lookup",
            json!({"city":"San Francisco"})
        )]
    );
}

#[test]
fn reducer_reconstructs_delta_only_tool_call_on_completion() {
    let response = response_from_events(
        ResponseFormat::Text,
        vec![
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 4,
                tool_call_index: 4,
                item_id: Some("item_delta_only".to_string()),
                tool_call_id: None,
                tool_name: Some("weather".to_string()),
                delta: "{\"zip\":\"94107\"}".to_string(),
            },
            CanonicalStreamEvent::OutputItemCompleted {
                output_index: 4,
                item: StreamOutputItemEnd::ToolCall {
                    item_id: Some("item_delta_only".to_string()),
                    tool_call_id: Some("call_delta_only".to_string()),
                    name: "weather".to_string(),
                    arguments_json_text: String::new(),
                },
            },
        ],
        Vec::new(),
    )
    .expect("response should be built");

    assert_eq!(
        response.output.content,
        vec![ContentPart::tool_call(
            "call_delta_only",
            "weather",
            json!({"zip":"94107"})
        )]
    );
}

#[test]
fn reducer_flushes_pending_tool_call_when_stream_ends_without_completion() {
    let response = response_from_events(
        ResponseFormat::Text,
        vec![
            CanonicalStreamEvent::OutputItemStarted {
                output_index: 2,
                item: StreamOutputItemStart::ToolCall {
                    item_id: Some("item_pending".to_string()),
                    tool_call_id: Some("call_pending".to_string()),
                    name: "search".to_string(),
                },
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                output_index: 2,
                tool_call_index: 2,
                item_id: Some("item_pending".to_string()),
                tool_call_id: None,
                tool_name: None,
                delta: "{\"q\":\"rust\"}".to_string(),
            },
        ],
        vec![CanonicalStreamEvent::Completed {
            finish_reason: FinishReason::ToolCalls,
        }],
    )
    .expect("response should be built");

    assert_eq!(
        response.output.content,
        vec![ContentPart::tool_call(
            "call_pending",
            "search",
            json!({"q":"rust"})
        )]
    );
}

#[test]
fn finalize_returns_upstream_error_when_failed_event_seen() {
    let error = response_from_events(
        ResponseFormat::Text,
        vec![CanonicalStreamEvent::Failed {
            message: "upstream exploded".to_string(),
        }],
        Vec::new(),
    )
    .expect_err("failed streams should return an error");

    match error {
        crate::provider_stream_runtime::StreamRuntimeError::Adapter { error, .. } => {
            assert_eq!(error.kind, AdapterErrorKind::Upstream);
            assert_eq!(error.message, "upstream exploded");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
