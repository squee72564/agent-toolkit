use std::collections::BTreeMap;

use agent_core::{
    Message, ProviderId, RawStreamPayload, RawStreamTransport, Request, ResponseFormat, ToolChoice,
};
use agent_providers::adapter::adapter_for;
use agent_transport::SseEvent;
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
