use std::collections::BTreeMap;
use std::time::Duration;

use agent_core::{
    Message, ProviderId, Request, ResolvedTransportOptions, ResponseFormat, ToolChoice,
};
use agent_providers::request_plan::{ProviderRequestPlan, TransportResponseFraming};
use agent_transport::{HttpResponseHead, HttpResponseMode};
use reqwest::{Method, StatusCode, header::HeaderMap};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::RuntimeErrorKind;
use crate::planner;
use crate::provider_client::ProviderClient;
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
        .execute_attempt(direct_execution_plan(
            &runtime,
            test_request("request-model", false).task_request(),
            Some("override-model"),
            crate::ExecutionOptions::default(),
        ))
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
        .execute_attempt(direct_execution_plan(
            &runtime,
            test_request(" ", false).task_request(),
            None,
            crate::ExecutionOptions::default(),
        ))
        .await;

    match attempt {
        ProviderAttemptOutcome::Failure { meta, error } => {
            assert_eq!(meta.model, "default-model");
            assert_eq!(meta.error_kind, Some(error.kind));
        }
        ProviderAttemptOutcome::Success { .. } => panic!("expected transport failure"),
    }
}

#[test]
fn direct_planner_fails_when_no_model_available() {
    let runtime = test_provider_runtime(ProviderId::OpenAi, "http://127.0.0.1:1", None);
    let error = planner::plan_direct_attempt(
        &ProviderClient::new(runtime),
        &test_request("", false).task_request(),
        None,
        &crate::ExecutionOptions::default(),
    )
    .expect_err("missing model must fail during planning");

    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
    assert!(error.message.contains("no model available"));
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
        .open_stream_attempt(direct_execution_plan(
            &runtime,
            test_request("", true).task_request(),
            Some("override-model"),
            crate::ExecutionOptions {
                response_mode: crate::ResponseMode::Streaming,
                ..crate::ExecutionOptions::default()
            },
        ))
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

#[test]
fn timeout_overrides_only_replace_attempt_local_timeout_fields() {
    let mut plan = ProviderRequestPlan {
        body: serde_json::json!({}),
        warnings: Vec::new(),
        method: Method::POST,
        response_framing: TransportResponseFraming::Json,
        endpoint_path_override: None,
        provider_headers: HeaderMap::new(),
        request_options: agent_transport::HttpRequestOptions::json_defaults()
            .with_request_timeout(Duration::from_secs(10))
            .with_stream_setup_timeout(Duration::from_secs(11))
            .with_stream_idle_timeout(Duration::from_secs(12)),
    };

    crate::provider_runtime::apply_timeout_overrides(
        &mut plan,
        &agent_core::ResolvedTransportOptions {
            request_id_header_override: None,
            route_extra_headers: BTreeMap::new(),
            attempt_extra_headers: BTreeMap::new(),
            timeout_overrides: crate::TransportTimeoutOverrides {
                request_timeout: Some(Duration::from_secs(3)),
                stream_setup_timeout: Some(Duration::from_secs(4)),
                stream_idle_timeout: Some(Duration::from_secs(5)),
            },
        },
    );

    assert_eq!(
        plan.request_options.request_timeout,
        Some(Duration::from_secs(3))
    );
    assert_eq!(
        plan.request_options.stream_setup_timeout,
        Some(Duration::from_secs(4))
    );
    assert_eq!(
        plan.request_options.stream_idle_timeout,
        Some(Duration::from_secs(5))
    );
}

#[test]
fn transport_metadata_shim_preserves_route_then_attempt_header_precedence() {
    let transport = crate::TransportOptions {
        request_id_header_override: Some("x-route-request-id".to_string()),
        extra_headers: BTreeMap::from([
            ("x-shared".to_string(), "route".to_string()),
            ("x-route-only".to_string(), "route-only".to_string()),
        ]),
    };
    let execution = crate::AttemptExecutionOptions::default().with_extra_headers(BTreeMap::from([
        ("x-shared".to_string(), "attempt".to_string()),
        ("x-attempt-only".to_string(), "attempt-only".to_string()),
    ]));

    let metadata = planner::build_transport_metadata_shim(&ResolvedTransportOptions {
        request_id_header_override: transport.request_id_header_override.clone(),
        route_extra_headers: transport.extra_headers.clone(),
        attempt_extra_headers: execution.extra_headers.clone(),
        timeout_overrides: execution.timeout_overrides.clone(),
    });

    assert_eq!(
        metadata
            .get("transport.request_id_header")
            .map(String::as_str),
        Some("x-route-request-id")
    );
    assert_eq!(
        metadata
            .get("transport.header.x-shared")
            .map(String::as_str),
        Some("attempt")
    );
    assert_eq!(
        metadata
            .get("transport.header.x-route-only")
            .map(String::as_str),
        Some("route-only")
    );
    assert_eq!(
        metadata
            .get("transport.header.x-attempt-only")
            .map(String::as_str),
        Some("attempt-only")
    );
}

fn direct_execution_plan(
    runtime: &ProviderRuntime,
    task: agent_core::TaskRequest,
    model_override: Option<&str>,
    execution: crate::ExecutionOptions,
) -> agent_core::ExecutionPlan {
    planner::plan_direct_attempt(
        &ProviderClient::new(runtime.clone()),
        &task,
        model_override,
        &execution,
    )
    .expect("planning should succeed")
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
    let instance_id = crate::Target::default_instance_for(provider);
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
