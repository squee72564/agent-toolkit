use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_core::types::{AdapterContext, AuthCredentials, AuthStyle, PlatformConfig, ProtocolKind};
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use super::{HttpTransport, RetryPolicy, TransportError};

#[derive(Debug)]
struct CapturedRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

#[derive(Clone)]
struct ScriptedResponse {
    status: StatusCode,
    headers: Vec<(String, String)>,
    body: String,
}

fn header_name(raw: &str) -> HeaderName {
    HeaderName::from_bytes(raw.as_bytes())
        .unwrap_or_else(|error| panic!("invalid header name {raw}: {error}"))
}

fn default_platform(auth_style: AuthStyle) -> PlatformConfig {
    PlatformConfig {
        protocol: ProtocolKind::OpenAI,
        base_url: "http://localhost".to_string(),
        auth_style,
        request_id_header: HeaderName::from_static("x-request-id"),
        default_headers: HeaderMap::new(),
    }
}

fn default_transport(retry_policy: RetryPolicy) -> HttpTransport {
    let client = reqwest::Client::new();
    HttpTransport::builder(client)
        .retry_policy(retry_policy)
        .timeout(Duration::from_secs(2))
        .build()
}

fn empty_context() -> AdapterContext {
    AdapterContext {
        metadata: BTreeMap::new(),
        auth_token: None,
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn invalid_data_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn parse_request_head(head: &str) -> io::Result<(String, String, HashMap<String, String>)> {
    let mut lines = head.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| invalid_data_error("missing request line"))?;

    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| invalid_data_error("missing request method"))?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| invalid_data_error("missing request path"))?
        .to_string();

    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }

        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| invalid_data_error(format!("invalid header line: {line}")))?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    Ok((method, path, headers))
}

async fn read_request(stream: &mut TcpStream) -> io::Result<CapturedRequest> {
    let mut buffer = Vec::new();
    let headers_end = loop {
        let mut chunk = [0_u8; 1024];
        let bytes_read = stream.read(&mut chunk).await?;
        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before full request",
            ));
        }

        buffer.extend_from_slice(&chunk[..bytes_read]);
        if let Some(index) = find_subsequence(&buffer, b"\r\n\r\n") {
            break index;
        }
    };

    let head = std::str::from_utf8(&buffer[..headers_end])
        .map_err(|error| invalid_data_error(format!("invalid utf8 in header block: {error}")))?;
    let (method, path, headers) = parse_request_head(head)?;

    let content_length = headers
        .get("content-length")
        .map(String::as_str)
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|error| invalid_data_error(format!("invalid content-length: {error}")))?;

    let body_start = headers_end + 4;
    let mut body = if buffer.len() > body_start {
        buffer[body_start..].to_vec()
    } else {
        Vec::new()
    };

    while body.len() < content_length {
        let mut chunk = vec![0_u8; content_length - body.len()];
        let bytes_read = stream.read(&mut chunk).await?;
        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before reading full body",
            ));
        }
        body.extend_from_slice(&chunk[..bytes_read]);
    }

    Ok(CapturedRequest {
        method,
        path,
        headers,
        body,
    })
}

async fn write_response(stream: &mut TcpStream, response: &ScriptedResponse) -> io::Result<()> {
    let reason = response.status.canonical_reason().unwrap_or("Unknown");
    let mut response_text = format!("HTTP/1.1 {} {}\r\n", response.status.as_u16(), reason);

    let mut has_content_type = false;
    for (name, value) in &response.headers {
        if name.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        response_text.push_str(name);
        response_text.push_str(": ");
        response_text.push_str(value);
        response_text.push_str("\r\n");
    }

    if !has_content_type {
        response_text.push_str("Content-Type: application/json\r\n");
    }
    response_text.push_str("Connection: close\r\n");
    response_text.push_str(&format!("Content-Length: {}\r\n\r\n", response.body.len()));
    response_text.push_str(&response.body);

    stream.write_all(response_text.as_bytes()).await?;
    stream.shutdown().await
}

async fn spawn_scripted_server(
    responses: Vec<ScriptedResponse>,
) -> io::Result<(
    String,
    Arc<Mutex<Vec<CapturedRequest>>>,
    JoinHandle<io::Result<()>>,
)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let recorded_clone = Arc::clone(&recorded);

    let mut queue: VecDeque<ScriptedResponse> = responses.into();
    let handle = tokio::spawn(async move {
        while let Some(response) = queue.pop_front() {
            let (mut stream, _) = listener.accept().await?;
            let request = read_request(&mut stream).await?;

            {
                let mut guard = recorded_clone
                    .lock()
                    .map_err(|_| io::Error::other("failed to acquire request capture lock"))?;
                guard.push(request);
            }

            write_response(&mut stream, &response).await?;
        }

        Ok(())
    });

    Ok((format!("http://{address}"), recorded, handle))
}

#[test]
fn retry_policy_backoff_caps_at_max() {
    let policy = RetryPolicy {
        max_attempts: 4,
        initial_backoff: Duration::from_millis(100),
        max_backoff: Duration::from_millis(300),
        retryable_status_codes: vec![],
    };

    assert_eq!(
        policy.backoff_duration_for_retry(0),
        Duration::from_millis(100)
    );
    assert_eq!(
        policy.backoff_duration_for_retry(1),
        Duration::from_millis(200)
    );
    assert_eq!(
        policy.backoff_duration_for_retry(2),
        Duration::from_millis(300)
    );
    assert_eq!(
        policy.backoff_duration_for_retry(10),
        Duration::from_millis(300)
    );
}

#[test]
fn build_header_config_applies_default_auth_and_metadata_headers() {
    let mut platform = default_platform(AuthStyle::Bearer);
    platform
        .default_headers
        .insert(header_name("x-default"), HeaderValue::from_static("base"));

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.request_id_header".to_string(),
        "x-trace-id".to_string(),
    );
    metadata.insert("transport.header.x-meta".to_string(), "meta".to_string());

    let ctx = AdapterContext {
        metadata,
        auth_token: Some(AuthCredentials::Token("secret-token".to_string())),
    };

    let transport = default_transport(RetryPolicy::default());
    let config = transport
        .build_header_config(&platform, &ctx)
        .unwrap_or_else(|error| panic!("expected valid header config: {error}"));

    assert_eq!(config.request_id_header, header_name("x-trace-id"));
    assert_eq!(
        config.headers.get("x-default"),
        Some(&HeaderValue::from_static("base"))
    );
    assert_eq!(
        config.headers.get("x-meta"),
        Some(&HeaderValue::from_static("meta"))
    );
    assert_eq!(
        config.headers.get(AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer secret-token"))
    );
}

#[test]
fn build_header_config_rejects_invalid_custom_header_name() {
    let platform = default_platform(AuthStyle::None);
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.header.invalid header".to_string(),
        "value".to_string(),
    );

    let ctx = AdapterContext {
        metadata,
        auth_token: None,
    };

    let transport = default_transport(RetryPolicy::default());
    let error = transport
        .build_header_config(&platform, &ctx)
        .err()
        .unwrap_or_else(|| panic!("expected invalid header name error"));

    assert!(matches!(error, TransportError::InvalidHeaderName));
}

#[test]
fn build_header_config_rejects_invalid_custom_header_value() {
    let platform = default_platform(AuthStyle::None);
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.header.x-bad".to_string(),
        "line1\nline2".to_string(),
    );

    let ctx = AdapterContext {
        metadata,
        auth_token: None,
    };

    let transport = default_transport(RetryPolicy::default());
    let error = transport
        .build_header_config(&platform, &ctx)
        .err()
        .unwrap_or_else(|| panic!("expected invalid header value error"));

    assert!(matches!(error, TransportError::InvalidHeaderValue));
}

#[derive(Serialize)]
struct ExampleBody<'a> {
    msg: &'a str,
}

#[tokio::test]
async fn post_json_value_preserves_non_success_status_and_extracts_request_id() {
    let responses = vec![ScriptedResponse {
        status: StatusCode::BAD_REQUEST,
        headers: vec![("x-trace-id".to_string(), "trace-42".to_string())],
        body: json!({"error": "bad request"}).to_string(),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses)
        .await
        .unwrap_or_else(|error| panic!("failed to start test server: {error}"));

    let mut platform = default_platform(AuthStyle::None);
    platform.base_url = base_url.clone();

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.request_id_header".to_string(),
        "x-trace-id".to_string(),
    );
    metadata.insert(
        "transport.header.x-custom".to_string(),
        "custom".to_string(),
    );
    let ctx = AdapterContext {
        metadata,
        auth_token: None,
    };

    let transport = default_transport(RetryPolicy::default());
    let response = transport
        .post_json_value(
            &platform,
            &format!("{base_url}/v1/test"),
            &ExampleBody { msg: "hello" },
            &ctx,
        )
        .await
        .unwrap_or_else(|error| panic!("post_json_value failed: {error}"));

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.request_id.as_deref(), Some("trace-42"));
    assert_eq!(response.body, json!({"error": "bad request"}));

    let server_result = handle
        .await
        .unwrap_or_else(|error| panic!("server join failed: {error}"));
    if let Err(error) = server_result {
        panic!("server failed: {error}");
    }

    let captured = recorded
        .lock()
        .unwrap_or_else(|_| panic!("failed to read captured requests"));
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].path, "/v1/test");
    assert_eq!(
        captured[0].headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        captured[0].headers.get("x-custom").map(String::as_str),
        Some("custom")
    );

    let body: Value = serde_json::from_slice(&captured[0].body)
        .unwrap_or_else(|error| panic!("captured request body was not valid json: {error}"));
    assert_eq!(body, json!({"msg": "hello"}));
}

#[tokio::test]
async fn get_json_retries_retryable_status_then_succeeds() {
    let responses = vec![
        ScriptedResponse {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: vec![],
            body: json!({"error": "try again"}).to_string(),
        },
        ScriptedResponse {
            status: StatusCode::OK,
            headers: vec![],
            body: json!({"ok": true}).to_string(),
        },
    ];
    let (base_url, recorded, handle) = spawn_scripted_server(responses)
        .await
        .unwrap_or_else(|error| panic!("failed to start test server: {error}"));

    let policy = RetryPolicy {
        max_attempts: 2,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(1),
        ..RetryPolicy::default()
    };

    let transport = default_transport(policy);
    let result: Value = transport
        .get_json(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/retry"),
            &empty_context(),
        )
        .await
        .unwrap_or_else(|error| panic!("get_json failed: {error}"));

    assert_eq!(result, json!({"ok": true}));

    let server_result = handle
        .await
        .unwrap_or_else(|error| panic!("server join failed: {error}"));
    if let Err(error) = server_result {
        panic!("server failed: {error}");
    }

    let captured = recorded
        .lock()
        .unwrap_or_else(|_| panic!("failed to read captured requests"));
    assert_eq!(captured.len(), 2);
    assert_eq!(captured[0].path, "/retry");
    assert_eq!(captured[1].path, "/retry");
}
