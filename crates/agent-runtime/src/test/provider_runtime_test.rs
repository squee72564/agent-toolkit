use std::collections::BTreeMap;

use agent_core::{Message, ProviderId, Request, ResponseFormat, ToolChoice};
use agent_transport::{HttpResponseHead, HttpResponseMode};
use reqwest::{StatusCode, header::HeaderMap};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::RuntimeErrorKind;
use crate::provider_runtime::{
    ProviderAttemptOutcome, ProviderRuntime, ProviderStreamAttemptOutcome,
    response_mode_mismatch_error,
};

#[test]
fn response_mode_mismatch_reports_protocol_violation_for_json_expectation() {
    let error = response_mode_mismatch_error(
        ProviderId::OpenAi,
        HttpResponseMode::Json,
        "SSE",
        &response_head(StatusCode::OK, Some("req_json_mismatch")),
    );

    assert_eq!(error.kind, RuntimeErrorKind::ProtocolViolation);
    assert_eq!(error.provider, Some(ProviderId::OpenAi));
    assert_eq!(error.status_code, Some(200));
    assert_eq!(error.request_id.as_deref(), Some("req_json_mismatch"));
    assert!(
        error.message.contains("expected JSON response, got SSE"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn response_mode_mismatch_reports_protocol_violation_for_sse_expectation() {
    let error = response_mode_mismatch_error(
        ProviderId::Anthropic,
        HttpResponseMode::Sse,
        "JSON",
        &response_head(StatusCode::CREATED, Some("req_sse_mismatch")),
    );

    assert_eq!(error.kind, RuntimeErrorKind::ProtocolViolation);
    assert_eq!(error.provider, Some(ProviderId::Anthropic));
    assert_eq!(error.status_code, Some(201));
    assert_eq!(error.request_id.as_deref(), Some("req_sse_mismatch"));
    assert!(
        error.message.contains("expected SSE response, got JSON"),
        "unexpected message: {}",
        error.message
    );
}

#[tokio::test]
async fn execute_attempt_uses_override_model_in_meta() {
    let runtime = test_provider_runtime(
        ProviderId::OpenAi,
        "http://127.0.0.1:1",
        Some("default-model"),
    );

    let attempt = runtime
        .execute_attempt(
            test_request("request-model", false),
            Some("override-model"),
            BTreeMap::new(),
        )
        .await;

    match attempt {
        ProviderAttemptOutcome::Failure { meta, error } => {
            assert_eq!(meta.model, "override-model");
            assert_eq!(meta.error_kind, Some(error.kind));
        }
        ProviderAttemptOutcome::Success { .. } => panic!("expected transport failure"),
    }
}

#[tokio::test]
async fn execute_attempt_uses_default_model_when_request_blank() {
    let runtime = test_provider_runtime(
        ProviderId::OpenAi,
        "http://127.0.0.1:1",
        Some("default-model"),
    );

    let attempt = runtime
        .execute_attempt(test_request(" ", false), None, BTreeMap::new())
        .await;

    match attempt {
        ProviderAttemptOutcome::Failure { meta, error } => {
            assert_eq!(meta.model, "default-model");
            assert_eq!(meta.error_kind, Some(error.kind));
        }
        ProviderAttemptOutcome::Success { .. } => panic!("expected transport failure"),
    }
}

#[tokio::test]
async fn execute_attempt_reports_unset_model_when_no_model_available() {
    let runtime = test_provider_runtime(ProviderId::OpenAi, "http://127.0.0.1:1", None);

    let attempt = runtime
        .execute_attempt(test_request("", false), None, BTreeMap::new())
        .await;

    match attempt {
        ProviderAttemptOutcome::Failure { meta, error } => {
            assert_eq!(meta.model, "<unset-model>");
            assert_eq!(meta.error_kind, Some(RuntimeErrorKind::Configuration));
            assert_eq!(meta.error_message.as_deref(), Some(error.message.as_str()));
            assert_eq!(error.kind, RuntimeErrorKind::Configuration);
        }
        ProviderAttemptOutcome::Success { .. } => panic!("expected configuration failure"),
    }
}

#[tokio::test]
async fn open_stream_attempt_reports_selected_model_and_response_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let runtime = test_provider_runtime(ProviderId::OpenAi, &base_url, Some("default-model"));

    let attempt = runtime
        .open_stream_attempt(
            test_request("", true),
            Some("override-model"),
            BTreeMap::new(),
        )
        .await;

    match attempt {
        ProviderStreamAttemptOutcome::Opened { mut stream, meta } => {
            assert_eq!(meta.model, "override-model");
            assert_eq!(meta.status_code, Some(200));
            assert_eq!(meta.request_id.as_deref(), Some("req_sse"));

            while stream
                .next_envelope()
                .await
                .expect("stream should advance")
                .is_some()
            {}

            let (response, final_http) = stream.finish().expect("stream should finalize");
            assert_eq!(response.model, "gpt-5-mini");
            assert_eq!(final_http.head.request_id.as_deref(), Some("req_sse"));
        }
        ProviderStreamAttemptOutcome::Failure { error, .. } => {
            panic!("expected opened stream, got error: {error}")
        }
    }
}

fn response_head(status: StatusCode, request_id: Option<&str>) -> HttpResponseHead {
    HttpResponseHead {
        status,
        headers: HeaderMap::new(),
        request_id: request_id.map(ToString::to_string),
    }
}

fn test_provider_runtime(
    provider: ProviderId,
    base_url: &str,
    default_model: Option<&str>,
) -> ProviderRuntime {
    let adapter = agent_providers::adapter::adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = agent_transport::HttpTransport::builder(client).build();
    let platform = adapter
        .platform_config(base_url.to_string())
        .expect("test platform should build");
    let instance_id = crate::Target::default_instance_for(provider);
    let mut config = crate::ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = crate::RegisteredProvider::new(instance_id.clone(), provider, config);

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

fn test_request(model_id: &str, stream: bool) -> Request {
    Request {
        model_id: model_id.to_string(),
        stream,
        messages: vec![Message::user_text("hello")],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: BTreeMap::new(),
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
