use std::collections::HashMap;

use agent_core::{CanonicalStreamEnvelope, CanonicalStreamEvent, ProviderId};
use agent_providers::adapter::adapter_for;
use agent_transport::HttpTransport;
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::{AgentToolkit, ExecutionOptions, MessageCreateInput, Route, Target};

use super::*;

#[tokio::test]
async fn direct_streaming_yields_envelopes_and_finishes_with_meta() {
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
    let client = test_streaming_provider_client(ProviderId::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield")
        .expect("first item should be ok");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(completion.meta.status_code, Some(200));
    assert_eq!(completion.meta.request_id.as_deref(), Some("req_sse"));
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello from stream")]
    );
}

#[tokio::test]
async fn direct_streaming_finish_after_drain_returns_completion() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"drained response\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderId::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open");

    while next_stream_item(&mut stream).await.is_some() {}

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("drained response")]
    );
}

#[tokio::test]
async fn routed_streaming_happy_path_finishes_with_response_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello from route\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderId::OpenAi)),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield")
        .expect("stream item should succeed");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(completion.meta.attempts.len(), 1);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello from route")]
    );
}

#[tokio::test]
async fn routed_streaming_explicit_task_api_uses_route_and_execution_options() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"explicit route stream\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let task = MessageCreateInput::user("hello explicit route")
        .into_task_request()
        .expect("task request should build");
    let route = Route::to(Target::new(ProviderId::OpenAi).with_model("gpt-5-mini"));
    let execution = ExecutionOptions {
        response_mode: crate::ResponseMode::Streaming,
        ..ExecutionOptions::default()
    };

    let mut stream = toolkit
        .streaming()
        .create_task(task, route, execution)
        .await
        .expect("explicit routed stream should open");

    let first = next_stream_item(&mut stream)
        .await
        .expect("stream should yield")
        .expect("stream item should succeed");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("explicit route stream")]
    );
}

#[tokio::test]
async fn routed_streaming_fail_fast_stops_on_planning_rejection_before_fallback() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                Target::default_instance_for(ProviderId::OpenAi),
                test_provider_client_with_streaming_support(
                    ProviderId::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                Target::default_instance_for(ProviderId::OpenRouter),
                test_provider_client(ProviderId::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let error = toolkit
        .streaming()
        .create(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderId::OpenAi))
                .with_fallback(Target::new(ProviderId::OpenRouter))
                .with_planning_rejection_policy(crate::PlanningRejectionPolicy::FailFast),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect_err("planning rejection must stop before fallback");

    assert_eq!(error.kind, crate::RuntimeErrorKind::Configuration);
    assert!(
        error.message.contains("does not support streaming"),
        "unexpected message: {}",
        error.message
    );
}

#[tokio::test]
async fn direct_text_stream_yields_text_chunks_and_finishes_with_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello \"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"world\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderId::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield")
            .expect("first text item should succeed"),
        "hello "
    );
    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield second delta")
            .expect("second text item should succeed"),
        "world"
    );

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello world")]
    );
}

#[tokio::test]
async fn routed_text_stream_yields_text_chunks_and_finishes_with_response_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello from route\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            crate::ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderId::OpenAi)),
            ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield")
            .expect("text item should succeed"),
        "hello from route"
    );

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(completion.meta.attempts.len(), 1);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello from route")]
    );
}

#[test]
fn text_stream_enqueues_multiple_text_deltas_from_one_envelope_in_order() {
    let mut pending = std::collections::VecDeque::new();

    crate::message_text_stream::MessageTextStream::enqueue_text_deltas(
        &mut pending,
        CanonicalStreamEnvelope {
            raw: agent_core::ProviderRawStreamEvent::from_sse(
                ProviderId::OpenAi,
                1,
                Some("response.synthetic".to_string()),
                None,
                None,
                r#"{"type":"response.synthetic"}"#,
            ),
            canonical: vec![
                CanonicalStreamEvent::ResponseStarted {
                    model: Some("gpt-5-mini".to_string()),
                    response_id: Some("resp_1".to_string()),
                },
                CanonicalStreamEvent::TextDelta {
                    output_index: 0,
                    content_index: 0,
                    item_id: Some("msg_1".to_string()),
                    delta: "hello ".to_string(),
                },
                CanonicalStreamEvent::TextDelta {
                    output_index: 0,
                    content_index: 1,
                    item_id: Some("msg_1".to_string()),
                    delta: "world".to_string(),
                },
            ],
        },
    );

    assert_eq!(
        pending.into_iter().collect::<Vec<_>>(),
        vec!["hello ".to_string(), "world".to_string()]
    );
}

#[tokio::test]
async fn text_stream_skips_non_text_envelopes_until_text_arrives() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"after setup\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderId::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield")
            .expect("text item should succeed"),
        "after setup"
    );
}

#[tokio::test]
async fn text_stream_finish_after_partial_consumption_preserves_full_response() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello \"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"again\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderId::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("first text chunk should be available")
            .expect("first text chunk should succeed"),
        "hello "
    );

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello again")]
    );
}

#[tokio::test]
async fn text_stream_surfaces_terminal_error_after_emitting_prior_text() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"partial text\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"error\":{\"message\":\"stream failed late\"},\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderId::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        next_text_stream_item(&mut stream)
            .await
            .expect("text stream should yield")
            .expect("first text item should succeed"),
        "partial text"
    );

    let poll_error = next_text_stream_item(&mut stream)
        .await
        .expect("text stream should surface terminal error")
        .expect_err("terminal item should be an error");
    assert_eq!(poll_error.kind, RuntimeErrorKind::Upstream);
    assert!(poll_error.message.contains("stream failed late"));

    let finish_error = stream
        .finish()
        .await
        .expect_err("finish should return the same error");
    assert_eq!(finish_error.kind, RuntimeErrorKind::Upstream);
    assert_eq!(finish_error.message, poll_error.message);
}

#[tokio::test]
async fn text_stream_completion_matches_envelope_stream_completion() {
    let body = concat!(
        "event: response.created\n",
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"same \"}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"response\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
    );
    let base_url_one = spawn_sse_stub("text/event-stream", body).await;
    let base_url_two = spawn_sse_stub("text/event-stream", body).await;
    let envelope_client =
        test_streaming_provider_client(ProviderId::OpenAi, &base_url_one, Some("gpt-5-mini"));
    let text_client =
        test_streaming_provider_client(ProviderId::OpenAi, &base_url_two, Some("gpt-5-mini"));

    let mut envelope_stream = envelope_client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("envelope stream should open");
    while next_stream_item(&mut envelope_stream).await.is_some() {}
    let envelope_completion = envelope_stream
        .finish()
        .await
        .expect("envelope completion should succeed");

    let mut text_stream = text_client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("text stream should open")
        .into_text_stream();
    while next_text_stream_item(&mut text_stream).await.is_some() {}
    let text_completion = text_stream
        .finish()
        .await
        .expect("text completion should succeed");

    assert_eq!(text_completion.response, envelope_completion.response);
    assert_eq!(text_completion.meta, envelope_completion.meta);
}

#[tokio::test]
async fn text_stream_finish_after_drain_returns_completion() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"finish after drain\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let client = test_streaming_provider_client(ProviderId::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    while next_text_stream_item(&mut stream).await.is_some() {}

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("finish after drain")]
    );
}

async fn next_stream_item(
    stream: &mut crate::MessageResponseStream,
) -> Option<Result<agent_core::CanonicalStreamEnvelope, crate::RuntimeError>> {
    stream.next().await
}

async fn next_text_stream_item(
    stream: &mut crate::MessageTextStream,
) -> Option<Result<String, crate::RuntimeError>> {
    stream.next().await
}

fn test_streaming_provider_client(
    provider: ProviderId,
    base_url: &str,
    default_model: Option<&str>,
) -> crate::provider_client::ProviderClient {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let platform = adapter
        .platform_config(base_url.to_string())
        .expect("test platform should build");
    let instance_id = crate::Target::default_instance_for(provider);
    let mut config = crate::ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = crate::RegisteredProvider::new(instance_id.clone(), provider, config);

    crate::provider_client::ProviderClient::new(crate::provider_runtime::ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    })
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
