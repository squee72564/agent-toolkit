use agent_core::ProviderKind;
use futures_util::StreamExt;

use crate::{test::{stream_test_fixtures::spawn_sse_stub, streaming_test_fixtures::test_streaming_provider_client}, MessageCreateInput};

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
    let client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open");

    let first = stream
        .next()
        .await
        .expect("stream should yield")
        .expect("first item should be ok");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
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
    let client =
        test_streaming_provider_client(ProviderKind::OpenAi, &base_url, Some("gpt-5-mini"));

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user("hello"))
        .await
        .expect("stream should open");

    while stream.next().await.is_some() {}

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("drained response")]
    );
}
