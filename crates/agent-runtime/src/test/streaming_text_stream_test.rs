use agent_core::{CanonicalStreamEnvelope, CanonicalStreamEvent, ProviderKind};
use futures_util::StreamExt;

use crate::message_text_stream::MessageTextStream;
use crate::{
    AgentToolkit, ExecutionOptions, MessageCreateInput, ProviderConfig, ProviderInstanceId,
    ResponseMode, Route, RuntimeErrorKind, Target,
};

use super::streaming_test_fixtures::*;
use super::*;

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
    let client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        stream
            .next()
            .await
            .expect("text stream should yield")
            .expect("first text item should succeed"),
        "hello "
    );
    assert_eq!(
        stream
            .next()
            .await
            .expect("text stream should yield second delta")
            .expect("second text item should succeed"),
        "world"
    );

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
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
            ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openai_default())),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        stream
            .next()
            .await
            .expect("text stream should yield")
            .expect("text item should succeed"),
        "hello from route"
    );

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.attempts.len(), 1);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello from route")]
    );
}

#[test]
fn text_stream_enqueues_multiple_text_deltas_from_one_envelope_in_order() {
    let mut pending = std::collections::VecDeque::new();

    MessageTextStream::enqueue_text_deltas(
        &mut pending,
        CanonicalStreamEnvelope {
            raw: agent_core::ProviderRawStreamEvent::from_sse(
                ProviderKind::OpenAi,
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
    let client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        stream
            .next()
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
    let client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        stream
            .next()
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
    let client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    assert_eq!(
        stream
            .next()
            .await
            .expect("text stream should yield")
            .expect("first text item should succeed"),
        "partial text"
    );

    let poll_error = stream
        .next()
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
    let failure_meta = executed_failure_meta(&finish_error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(failure_meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(failure_meta.selected_model, "gpt-5-mini");
    assert_eq!(failure_meta.attempts.len(), 1);
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
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url_one, Some("gpt-5-mini"));
    let text_client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url_two, Some("gpt-5-mini"));

    let mut envelope_stream = envelope_client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("envelope stream should open");
    while envelope_stream.next().await.is_some() {}
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
    while text_stream.next().await.is_some() {}
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
    let client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open")
        .into_text_stream();

    while stream.next().await.is_some() {}

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("finish after drain")]
    );
}
