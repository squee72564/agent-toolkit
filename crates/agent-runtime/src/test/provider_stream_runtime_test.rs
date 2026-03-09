use std::collections::BTreeMap;

use agent_core::{
    CanonicalStreamEnvelope, CanonicalStreamEvent, ContentPart, FinishReason, Message, ProviderId,
    RawStreamPayload, RawStreamTransport, Request, Response, ResponseFormat, RuntimeWarning,
    StreamOutputItemEnd, StreamOutputItemStart, ToolChoice,
};
use agent_providers::adapter::adapter_for;
use agent_providers::error::AdapterErrorKind;
use agent_transport::SseEvent;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::provider_runtime::{ProviderAttemptOutcome, ProviderRuntime};
use crate::provider_stream_runtime::ProviderStreamRuntime;
use crate::{MessageCreateInput, RuntimeErrorKind};

#[test]
fn wrap_sse_event_assigns_monotonic_sequences() {
    let mut runtime = ProviderStreamRuntime::new(ProviderId::OpenAi);

    let first = runtime.wrap_sse_event(SseEvent {
        event: Some("response.created".to_string()),
        data: r#"{"type":"response.created"}"#.to_string(),
        id: Some("evt-1".to_string()),
        retry: Some(10),
    });
    let second = runtime.wrap_sse_event(SseEvent {
        event: Some("response.completed".to_string()),
        data: "[DONE]".to_string(),
        id: Some("evt-2".to_string()),
        retry: None,
    });

    assert_eq!(first.sequence, 1);
    assert_eq!(second.sequence, 2);
}

#[test]
fn wrap_sse_event_preserves_transport_metadata_and_payload_shape() {
    let mut runtime = ProviderStreamRuntime::new(ProviderId::Anthropic);

    let raw = runtime.wrap_sse_event(SseEvent {
        event: Some("message_start".to_string()),
        data: r#"{"message":{"id":"msg_1"}}"#.to_string(),
        id: Some("sse-1".to_string()),
        retry: Some(250),
    });

    assert_eq!(raw.provider, ProviderId::Anthropic);
    assert_eq!(
        raw.transport,
        RawStreamTransport::Sse {
            event: Some("message_start".to_string()),
            id: Some("sse-1".to_string()),
            retry: Some(250),
        }
    );
    assert!(matches!(raw.payload, RawStreamPayload::Json { .. }));
}

#[tokio::test]
async fn current_non_streaming_api_rejects_stream_requests() {
    let client = super::test_provider_client(ProviderId::OpenAi);

    let error = client
        .create(
            MessageCreateInput::user("hello")
                .with_model("gpt-5-mini")
                .with_stream(true),
        )
        .await
        .expect_err("stream=true should be rejected on the current response API");

    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
    assert!(
        error.message.contains("stream=true is not supported"),
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
    let runtime = test_provider_runtime(ProviderId::OpenAi, &base_url, Some("gpt-5-mini"));

    let attempt = runtime
        .execute_attempt(
            Request {
                model_id: String::new(),
                stream: true,
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
            None,
            BTreeMap::new(),
        )
        .await;

    match attempt {
        ProviderAttemptOutcome::Success { response, meta } => {
            assert_eq!(meta.status_code, Some(200));
            assert_eq!(meta.request_id.as_deref(), Some("req_sse"));
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

fn response_from_events(
    response_format: ResponseFormat,
    streamed_events: Vec<CanonicalStreamEvent>,
    final_events: Vec<CanonicalStreamEvent>,
) -> Result<Response, crate::provider_stream_runtime::StreamRuntimeError> {
    ProviderStreamRuntime::response_from_events_for_test(
        ProviderId::OpenAi,
        &response_format,
        Vec::new(),
        vec![CanonicalStreamEnvelope {
            raw: ProviderStreamRuntime::new(ProviderId::OpenAi).wrap_sse_event(SseEvent {
                event: Some("test".to_string()),
                data: "{}".to_string(),
                id: Some("evt_test".to_string()),
                retry: None,
            }),
            canonical: streamed_events.clone(),
        }],
        &streamed_events,
        final_events,
    )
}

fn test_provider_runtime(
    provider: ProviderId,
    base_url: &str,
    default_model: Option<&str>,
) -> ProviderRuntime {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = agent_transport::HttpTransport::builder(client).build();
    let platform = adapter
        .platform_config(base_url.to_string())
        .expect("test platform should build");

    ProviderRuntime {
        provider,
        adapter,
        platform,
        auth_token: "test-key".to_string(),
        default_model: default_model.map(ToString::to_string),
        transport,
        observer: None,
    }
}

async fn spawn_sse_stub(content_type: &str, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let content_type = content_type.to_string();
    let body = body.to_string();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept test stream");
        let mut scratch = [0_u8; 8192];
        let _ = stream.read(&mut scratch).await;

        let http = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nx-request-id: req_sse\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(http.as_bytes()).await;
        let _ = stream.shutdown().await;
    });

    format!("http://{addr}")
}
