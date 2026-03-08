use std::collections::BTreeMap;
use std::pin::Pin;

use agent_core::{Message, ProviderId, Request, ResponseFormat, ToolChoice};
use agent_providers::adapter::adapter_for;
use agent_transport::HttpTransport;
use futures_core::Stream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::{AgentToolkit, MessageCreateInput, SendOptions, Target};

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
async fn direct_streaming_create_request_requires_stream_true() {
    let client = test_provider_client(ProviderId::OpenAi);

    let error = client
        .streaming()
        .create_request(Request {
            model_id: "gpt-5-mini".to_string(),
            stream: false,
            messages: vec![Message::user_text("hello")],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            response_format: ResponseFormat::Text,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            stop: Vec::new(),
            metadata: BTreeMap::new(),
        })
        .await
        .expect_err("non-stream request should fail");

    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
    assert!(error.message.contains("request.stream = true"));
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
            SendOptions::for_target(Target::new(ProviderId::OpenAi)),
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

async fn next_stream_item(
    stream: &mut crate::MessageResponseStream,
) -> Option<Result<agent_core::CanonicalStreamEnvelope, crate::RuntimeError>> {
    std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
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

    crate::provider_client::ProviderClient::new(crate::provider_runtime::ProviderRuntime {
        provider,
        adapter,
        platform,
        auth_token: "test-key".to_string(),
        default_model: default_model.map(ToString::to_string),
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
