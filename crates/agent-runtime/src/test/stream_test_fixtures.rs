use agent_core::{
    CanonicalStreamEnvelope, CanonicalStreamEvent, ProviderKind, Response, ResponseFormat,
};
use agent_providers::adapter::adapter_for;
use agent_transport::SseEvent;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::provider_runtime::ProviderRuntime;
use crate::provider_stream_runtime::ProviderStreamRuntime;

pub(super) fn response_from_events(
    response_format: ResponseFormat,
    streamed_events: Vec<CanonicalStreamEvent>,
    final_events: Vec<CanonicalStreamEvent>,
) -> Result<Response, crate::provider_stream_runtime::StreamRuntimeError> {
    ProviderStreamRuntime::response_from_events_for_test(
        ProviderKind::OpenAi,
        &response_format,
        Vec::new(),
        vec![CanonicalStreamEnvelope {
            raw: ProviderStreamRuntime::new(ProviderKind::OpenAi).wrap_sse_event(SseEvent {
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

pub(super) fn test_provider_runtime(
    provider: ProviderKind,
    base_url: &str,
    default_model: Option<&str>,
) -> ProviderRuntime {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = agent_transport::HttpTransport::builder(client).build();
    let instance_id = crate::test::default_instance_id(provider);
    let mut config = crate::ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = crate::RegisteredProvider::new(instance_id.clone(), provider, config);
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

    ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    }
}

pub(super) async fn spawn_sse_stub(content_type: &str, body: &str) -> String {
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
