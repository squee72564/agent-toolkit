use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use agent_core::{
    Message, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId, ProviderKind, Response,
    ResponseFormat, TaskRequest, ToolChoice,
};
use agent_providers::adapter::{ProviderAdapter, adapter_for};
use agent_providers::error::{AdapterError, ProviderErrorInfo};
use agent_providers::request_plan::ProviderRequestPlan;
use agent_providers::stream_projector::ProviderStreamProjector;
use agent_transport::HttpRequestOptions;
use agent_transport::{HttpResponseHead, TransportResponseFraming};
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::planner;
use crate::provider::ProviderClient;
use crate::provider_runtime::{
    ProviderAttemptOutcome, ProviderRuntime, ProviderStreamAttemptOutcome,
    response_mode_mismatch_error,
};
use crate::test::default_instance_id;
use crate::{
    AttemptExecutionOptions, AttemptSpec, ExecutionOptions, ProviderConfig, ProviderInstanceId,
    RegisteredProvider, ResponseMode, RuntimeErrorKind, Target, TransportOptions,
    TransportTimeoutOverrides,
};

#[test]
fn response_mode_mismatch_reports_protocol_violation_for_json_expectation() {
    let error = response_mode_mismatch_error(
        ProviderKind::OpenAi,
        TransportResponseFraming::Json,
        "SSE",
        &response_head(StatusCode::OK, Some("req_json_mismatch")),
    );

    assert_eq!(error.kind, RuntimeErrorKind::ProtocolViolation);
    assert_eq!(error.provider, Some(ProviderKind::OpenAi));
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
        ProviderKind::Anthropic,
        TransportResponseFraming::Sse,
        "JSON",
        &response_head(StatusCode::CREATED, Some("req_sse_mismatch")),
    );

    assert_eq!(error.kind, RuntimeErrorKind::ProtocolViolation);
    assert_eq!(error.provider, Some(ProviderKind::Anthropic));
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
        ProviderKind::OpenAi,
        "http://127.0.0.1:1",
        Some("default-model"),
    );

    let attempt = runtime
        .execute_attempt(direct_execution_plan(
            &runtime,
            test_task_request(),
            Some("override-model"),
            ExecutionOptions::default(),
        ))
        .await;

    match attempt {
        ProviderAttemptOutcome::Failure {
            selected_model,
            error,
        } => {
            assert_eq!(selected_model, "override-model");
            assert_eq!(error.kind, RuntimeErrorKind::Transport);
        }
        ProviderAttemptOutcome::Success { .. } => panic!("expected transport failure"),
    }
}

#[tokio::test]
async fn execute_attempt_uses_default_model_when_request_blank() {
    let runtime = test_provider_runtime(
        ProviderKind::OpenAi,
        "http://127.0.0.1:1",
        Some("default-model"),
    );

    let attempt = runtime
        .execute_attempt(direct_execution_plan(
            &runtime,
            test_task_request(),
            None,
            ExecutionOptions::default(),
        ))
        .await;

    match attempt {
        ProviderAttemptOutcome::Failure {
            selected_model,
            error,
        } => {
            assert_eq!(selected_model, "default-model");
            assert_eq!(error.kind, RuntimeErrorKind::Transport);
        }
        ProviderAttemptOutcome::Success { .. } => panic!("expected transport failure"),
    }
}

#[test]
fn direct_planner_fails_when_no_model_available() {
    let runtime = test_provider_runtime(ProviderKind::OpenAi, "http://127.0.0.1:1", None);
    let error = planner::plan_direct_attempt(
        &ProviderClient::new(runtime),
        &test_task_request(),
        &AttemptSpec::to(Target::new(default_instance_id(ProviderKind::OpenAi))),
        &ExecutionOptions::default(),
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
    let runtime = test_provider_runtime(ProviderKind::OpenAi, &base_url, Some("default-model"));

    let attempt = runtime
        .open_stream_attempt(direct_execution_plan(
            &runtime,
            test_task_request(),
            Some("override-model"),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        ))
        .await;

    match attempt {
        ProviderStreamAttemptOutcome::Opened {
            mut stream,
            selected_model,
            status_code,
            request_id,
        } => {
            assert_eq!(selected_model, "override-model");
            assert_eq!(status_code, Some(200));
            assert_eq!(request_id.as_deref(), Some("req_sse"));

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

#[tokio::test]
async fn open_stream_attempt_copies_adapter_selected_method_path_headers_and_framing() {
    let (base_url, captured) = spawn_capturing_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let runtime = test_provider_runtime_with_adapter(
        &TEST_STREAM_TRANSPORT_ADAPTER,
        &base_url,
        Some("default-model"),
    );

    let attempt = runtime
        .open_stream_attempt(direct_execution_plan(
            &runtime,
            test_task_request(),
            Some("override-model"),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        ))
        .await;

    match attempt {
        ProviderStreamAttemptOutcome::Opened { mut stream, .. } => {
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

    let captured = captured
        .lock()
        .expect("captured request lock")
        .clone()
        .expect("request should be captured");
    assert_eq!(captured.method, "GET");
    assert_eq!(captured.path, "/custom/stream");
    assert_eq!(
        captured.headers.get("x-provider-test").map(String::as_str),
        Some("provider")
    );
    assert_eq!(
        captured.headers.get("accept").map(String::as_str),
        Some("text/event-stream")
    );
}

#[test]
fn planner_resolves_transport_headers_timeouts_and_retry_policy() {
    let transport = TransportOptions {
        request_id_header_override: Some("x-route-request-id".to_string()),
        extra_headers: BTreeMap::from([
            ("x-shared".to_string(), "route".to_string()),
            ("x-route-only".to_string(), "route-only".to_string()),
        ]),
    };
    let execution = AttemptExecutionOptions::default()
        .with_timeout_overrides(TransportTimeoutOverrides {
            request_timeout: Some(Duration::from_secs(3)),
            stream_setup_timeout: Some(Duration::from_secs(4)),
            stream_idle_timeout: Some(Duration::from_secs(5)),
        })
        .with_extra_headers(BTreeMap::from([
            ("x-shared".to_string(), "attempt".to_string()),
            ("x-attempt-only".to_string(), "attempt-only".to_string()),
        ]));

    let runtime = test_provider_runtime_with(
        ProviderKind::OpenAi,
        "http://127.0.0.1:1",
        Some("model"),
        |config: ProviderConfig| {
            config
                .with_request_timeout(Duration::from_secs(10))
                .with_stream_timeout(Duration::from_secs(11))
                .with_retry_policy(agent_core::RetryPolicy {
                    max_attempts: 7,
                    initial_backoff: Duration::from_millis(5),
                    max_backoff: Duration::from_millis(20),
                    retryable_status_codes: vec![StatusCode::TOO_MANY_REQUESTS],
                })
        },
    );

    let execution_plan = planner::plan_routed_attempt(
        &ProviderClient::new(runtime),
        &AttemptSpec::to(Target::new(ProviderInstanceId::openai_default()).with_model("model"))
            .with_execution(execution),
        &test_task_request(),
        &ExecutionOptions {
            transport,
            ..ExecutionOptions::default()
        },
    )
    .expect("planning should succeed");

    let resolved = execution_plan.transport;
    assert_eq!(
        resolved.request_id_header_override.as_deref(),
        Some("x-route-request-id")
    );
    assert_eq!(resolved.route_extra_headers["x-shared"], "route");
    assert_eq!(resolved.route_extra_headers["x-route-only"], "route-only");
    assert_eq!(resolved.attempt_extra_headers["x-shared"], "attempt");
    assert_eq!(
        resolved.attempt_extra_headers["x-attempt-only"],
        "attempt-only"
    );
    assert_eq!(
        resolved.timeouts.request_timeout,
        Some(Duration::from_secs(3))
    );
    assert_eq!(
        resolved.timeouts.stream_setup_timeout,
        Some(Duration::from_secs(4))
    );
    assert_eq!(
        resolved.timeouts.stream_idle_timeout,
        Some(Duration::from_secs(5))
    );
    assert_eq!(resolved.retry_policy.max_attempts, 7);
}

fn direct_execution_plan(
    runtime: &ProviderRuntime,
    task: agent_core::TaskRequest,
    model_override: Option<&str>,
    execution: ExecutionOptions,
) -> agent_core::ExecutionPlan {
    let attempt = AttemptSpec::to(Target {
        instance: runtime.instance_id.clone(),
        model: model_override.map(ToString::to_string),
    });
    planner::plan_direct_attempt(
        &ProviderClient::new(runtime.clone()),
        &task,
        &attempt,
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
    provider: ProviderKind,
    base_url: &str,
    default_model: Option<&str>,
) -> ProviderRuntime {
    test_provider_runtime_with(provider, base_url, default_model, |config| config)
}

fn test_provider_runtime_with_adapter(
    adapter: &'static dyn ProviderAdapter,
    base_url: &str,
    default_model: Option<&str>,
) -> ProviderRuntime {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = agent_transport::HttpTransport::builder(client).build();
    let instance_id = default_instance_id(adapter.kind());
    let mut config = ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = RegisteredProvider::new(instance_id.clone(), adapter.kind(), config);
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

    ProviderRuntime {
        instance_id,
        kind: adapter.kind(),
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    }
}

fn test_provider_runtime_with(
    provider: ProviderKind,
    base_url: &str,
    default_model: Option<&str>,
    configure: impl FnOnce(ProviderConfig) -> ProviderConfig,
) -> ProviderRuntime {
    let adapter = agent_providers::adapter::adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = agent_transport::HttpTransport::builder(client).build();
    let instance_id = default_instance_id(provider);
    let mut config = ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    config = configure(config);
    let registered = RegisteredProvider::new(instance_id.clone(), provider, config);
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

fn test_task_request() -> TaskRequest {
    TaskRequest {
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

#[derive(Debug, Clone)]
struct CapturedHttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
}

#[derive(Debug)]
struct StreamTransportContractAdapter;

static TEST_STREAM_TRANSPORT_ADAPTER: StreamTransportContractAdapter =
    StreamTransportContractAdapter;

impl ProviderAdapter for StreamTransportContractAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::GenericOpenAiCompatible
    }

    fn descriptor(&self) -> &ProviderDescriptor {
        static DESCRIPTOR: LazyLock<ProviderDescriptor> = LazyLock::new(|| ProviderDescriptor {
            kind: ProviderKind::GenericOpenAiCompatible,
            family: ProviderFamilyId::OpenAiCompatible,
            protocol: agent_core::ProtocolKind::OpenAI,
            default_base_url: "https://example.invalid",
            endpoint_path: "/v1/default-stream",
            default_auth_style: agent_core::AuthStyle::Bearer,
            default_request_id_header: HeaderName::from_static("x-request-id"),
            default_headers: HeaderMap::new(),
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_family_native_options: false,
                supports_provider_native_options: false,
            },
        });
        &DESCRIPTOR
    }

    fn plan_request(
        &self,
        _execution: &agent_core::ExecutionPlan,
    ) -> Result<ProviderRequestPlan, AdapterError> {
        let mut provider_headers = HeaderMap::new();
        provider_headers.insert(
            HeaderName::from_static("x-provider-test"),
            HeaderValue::from_static("provider"),
        );

        Ok(ProviderRequestPlan {
            body: json!({}),
            warnings: Vec::new(),
            method: reqwest::Method::GET,
            response_framing: TransportResponseFraming::Sse,
            endpoint_path_override: Some("/custom/stream".to_string()),
            provider_headers,
            request_options: HttpRequestOptions::sse_defaults(),
        })
    }

    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        adapter_for(ProviderKind::OpenAi).decode_response_json(body, requested_format)
    }

    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        adapter_for(ProviderKind::OpenAi).decode_error(body)
    }

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        adapter_for(ProviderKind::OpenAi).create_stream_projector()
    }
}

async fn spawn_capturing_sse_stub(
    content_type: &str,
    body: &str,
) -> (String, Arc<Mutex<Option<CapturedHttpRequest>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let content_type = content_type.to_string();
    let body = body.to_string();
    let captured = Arc::new(Mutex::new(None));
    let captured_request = Arc::clone(&captured);

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept test stream");
        let mut scratch = [0_u8; 8192];
        let read = stream.read(&mut scratch).await.expect("read request");
        let request = String::from_utf8_lossy(&scratch[..read]).to_string();
        let parsed = parse_captured_request(&request);
        *captured_request.lock().expect("capture request") = Some(parsed);

        let http = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nx-request-id: req_sse\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(http.as_bytes()).await;
        let _ = stream.shutdown().await;
    });

    (format!("http://{addr}"), captured)
}

fn parse_captured_request(raw: &str) -> CapturedHttpRequest {
    let mut lines = raw.split("\r\n");
    let request_line = lines.next().expect("request line");
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().expect("request method").to_string();
    let path = request_parts.next().expect("request path").to_string();
    let mut headers = BTreeMap::new();

    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    CapturedHttpRequest {
        method,
        path,
        headers,
    }
}
