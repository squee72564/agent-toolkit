use std::collections::BTreeMap;
use std::time::Duration;

use agent_core::{
    AnthropicFamilyOptions, AnthropicOptions, OpenAiCompatibleOptions, OpenAiOptions,
    OpenRouterOptions,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::{MessageCreateInput, anthropic, openai, openrouter};

const OPENAI_SUCCESS_BODY: &str = include_str!(
    "../../../agent-providers/data/openai/responses/decoded/basic_chat/gpt-5-mini.json"
);
const ANTHROPIC_SUCCESS_BODY: &str = include_str!(
    "../../../agent-providers/data/anthropic/responses/decoded/basic_chat/claude-sonnet-4-6.json"
);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct CapturedRequest {
    path: String,
    body: Value,
}

async fn with_timeout<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(REQUEST_TIMEOUT, future)
        .await
        .expect("request timed out in test")
}

async fn spawn_json_capture_stub(
    response_content_type: &str,
    response_body: &str,
) -> (String, tokio::sync::oneshot::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind capture listener");
    let addr = listener.local_addr().expect("listener local addr");
    let response_content_type = response_content_type.to_string();
    let response_body = response_body.to_string();
    let (sender, receiver) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept request");
        let request_bytes = read_http_request(&mut stream).await;
        let request_text = String::from_utf8(request_bytes).expect("request should be utf8");
        let captured = parse_captured_request(&request_text);
        let _ = sender.send(captured);

        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {response_content_type}\r\ncontent-length: {}\r\nx-request-id: req_native_options\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write response");
        let _ = stream.shutdown().await;
    });

    (format!("http://{addr}"), receiver)
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut request_bytes = Vec::new();
    let mut scratch = [0_u8; 1024];

    loop {
        let read = stream.read(&mut scratch).await.expect("read request");
        assert!(read > 0, "request ended before headers");
        request_bytes.extend_from_slice(&scratch[..read]);

        if let Some(header_end) = find_header_end(&request_bytes) {
            let content_length = parse_content_length(&request_bytes[..header_end]);
            let body_end = header_end + 4 + content_length;

            while request_bytes.len() < body_end {
                let read = stream.read(&mut scratch).await.expect("read body");
                assert!(read > 0, "request ended before body");
                request_bytes.extend_from_slice(&scratch[..read]);
            }

            return request_bytes[..body_end].to_vec();
        }
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &[u8]) -> usize {
    let header_text = String::from_utf8(headers.to_vec()).expect("headers should be utf8");
    header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("content-length should parse"),
                )
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn parse_captured_request(raw: &str) -> CapturedRequest {
    let mut lines = raw.split("\r\n");
    let request_line = lines.next().expect("request line");
    let mut request_parts = request_line.split_whitespace();
    let _method = request_parts.next().expect("request method");
    let path = request_parts.next().expect("request path").to_string();
    let (_, body) = raw
        .split_once("\r\n\r\n")
        .expect("request should include body");

    CapturedRequest {
        path,
        body: serde_json::from_str(body).expect("request body should be valid json"),
    }
}

#[tokio::test]
async fn openai_direct_helper_normalizes_typed_native_options_into_payload() {
    let (base_url, captured) =
        spawn_json_capture_stub("application/json", OPENAI_SUCCESS_BODY).await;
    let client = openai()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("gpt-5-mini")
        .build()
        .expect("build openai client");

    let _response = with_timeout(client.create_with_openai_options(
        MessageCreateInput::user("hello"),
        Some("gpt-5.1".to_string()),
        Some(OpenAiCompatibleOptions {
            parallel_tool_calls: Some(true),
            reasoning: Some(json!({ "effort": "medium" })),
        }),
        Some(OpenAiOptions {
            service_tier: Some("priority".to_string()),
            store: Some(true),
        }),
    ))
    .await
    .expect("request should succeed");

    let captured = captured.await.expect("captured request should arrive");
    assert_eq!(captured.path, "/v1/responses");
    assert_eq!(captured.body["model"], "gpt-5.1");
    assert_eq!(captured.body["parallel_tool_calls"], true);
    assert_eq!(captured.body["reasoning"], json!({ "effort": "medium" }));
    assert_eq!(captured.body["service_tier"], "priority");
    assert_eq!(captured.body["store"], true);
}

#[tokio::test]
async fn openrouter_direct_helper_normalizes_typed_native_options_into_payload() {
    let (base_url, captured) =
        spawn_json_capture_stub("application/json", OPENAI_SUCCESS_BODY).await;
    let client = openrouter()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("openai/gpt-5-mini")
        .build()
        .expect("build openrouter client");

    let _response = with_timeout(client.create_with_openrouter_options(
        MessageCreateInput::user("hello"),
        Some("openai/gpt-5.1".to_string()),
        Some(OpenAiCompatibleOptions {
            parallel_tool_calls: Some(true),
            reasoning: Some(json!({ "effort": "low" })),
        }),
        Some(OpenRouterOptions {
            fallback_models: vec!["openai/gpt-4.1-mini".to_string()],
            provider_preferences: Some(json!({ "order": ["openai"] })),
            plugins: vec![json!({ "id": "web" })],
            frequency_penalty: Some(0.25),
            presence_penalty: Some(0.5),
            logit_bias: None,
            logprobs: Some(true),
            top_logprobs: Some(3),
            seed: Some(42),
            user: Some("user-123".to_string()),
            session_id: Some("session-456".to_string()),
            trace: Some(json!({ "trace_id": "trace-789" })),
            route: Some("fallback".to_string()),
            modalities: Some(vec!["text".to_string()]),
            image_config: None,
            debug: Some(json!({ "enabled": true })),
        }),
    ))
    .await
    .expect("request should succeed");

    let captured = captured.await.expect("captured request should arrive");
    assert_eq!(captured.path, "/v1/responses");
    assert_eq!(
        captured.body["models"],
        json!(["openai/gpt-5.1", "openai/gpt-4.1-mini"])
    );
    assert_eq!(captured.body["parallel_tool_calls"], true);
    assert_eq!(captured.body["reasoning"], json!({ "effort": "low" }));
    assert_eq!(captured.body["provider"], json!({ "order": ["openai"] }));
    assert_eq!(captured.body["plugins"], json!([{ "id": "web" }]));
    assert_eq!(captured.body["frequency_penalty"], json!(0.25));
    assert_eq!(captured.body["presence_penalty"], json!(0.5));
    assert_eq!(captured.body["logprobs"], true);
    assert_eq!(captured.body["top_logprobs"], 3);
    assert_eq!(captured.body["seed"], 42);
    assert_eq!(captured.body["user"], "user-123");
    assert_eq!(captured.body["session_id"], "session-456");
    assert_eq!(captured.body["trace"], json!({ "trace_id": "trace-789" }));
    assert_eq!(captured.body["route"], "fallback");
    assert_eq!(captured.body["modalities"], json!(["text"]));
    assert_eq!(captured.body["debug"], json!({ "enabled": true }));
}

#[tokio::test]
async fn anthropic_direct_helper_normalizes_typed_native_options_into_payload() {
    let (base_url, captured) =
        spawn_json_capture_stub("application/json", ANTHROPIC_SUCCESS_BODY).await;
    let client = anthropic()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("claude-sonnet-4-6")
        .build()
        .expect("build anthropic client");

    let _response = with_timeout(client.create_with_anthropic_options(
        MessageCreateInput::user("hello"),
        Some("claude-opus-4-1-20250805".to_string()),
        Some(AnthropicFamilyOptions {
            thinking: Some(json!({ "type": "enabled", "budget_tokens": 128 })),
        }),
        Some(AnthropicOptions { top_k: Some(8) }),
    ))
    .await
    .expect("request should succeed");

    let captured = captured.await.expect("captured request should arrive");
    assert_eq!(captured.path, "/v1/messages");
    assert_eq!(captured.body["model"], "claude-opus-4-1-20250805");
    assert_eq!(
        captured.body["thinking"],
        json!({ "type": "enabled", "budget_tokens": 128 })
    );
    assert_eq!(captured.body["top_k"], 8);
}

#[test]
fn openrouter_native_options_builder_retains_typed_route_field() {
    let options = OpenRouterOptions::new().with_route("fallback");
    let mut expected = BTreeMap::new();
    expected.insert("route".to_string(), Value::String("fallback".to_string()));

    let serialized = serde_json::to_value(options).expect("options should serialize");
    let object = serialized
        .as_object()
        .expect("serialized options should be an object");

    assert_eq!(
        object.get("route"),
        expected.get("route"),
        "typed provider options should preserve the explicit route selector",
    );
}
