use std::time::Duration;

use agent_core::{
    AnthropicCacheControl, AnthropicCacheControlTTL, AnthropicCacheControlType,
    AnthropicFamilyOptions, AnthropicOptions, AnthropicOutputConfig, AnthropicOutputEffort,
    AnthropicServiceTier, AnthropicThinking, AnthropicThinkingBudget, AnthropicToolChoiceOptions,
    OpenAiCompatibleOptions, OpenAiCompatibleReasoning, OpenAiCompatibleReasoningEffort,
    OpenAiOptions, OpenAiPromptCacheRetention, OpenAiServiceTier, OpenAiTextOptions,
    OpenAiTextVerbosity, OpenAiTruncation, OpenRouterImageConfigValue, OpenRouterOptions,
    OpenRouterPlugin, OpenRouterTextOptions, OpenRouterTextVerbosity, OpenRouterTrace,
    OpenRouterWebPlugin,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::{MessageCreateInput, anthropic, openai, openrouter};

const OPENAI_SUCCESS_BODY: &str = include_str!(
    "../../../../agent-providers/data/openai/responses/decoded/basic_chat/gpt-5-mini.json"
);
const ANTHROPIC_SUCCESS_BODY: &str = include_str!(
    "../../../../agent-providers/data/anthropic/responses/decoded/basic_chat/claude-sonnet-4-6.json"
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

fn assert_json_number_close(actual: &Value, expected: f64) {
    let actual = actual
        .as_f64()
        .expect("expected JSON number in captured request body");
    let delta = (actual - expected).abs();
    assert!(
        delta < 1e-6,
        "expected numeric value close to {expected}, got {actual} (delta {delta})"
    );
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
            reasoning: Some(OpenAiCompatibleReasoning {
                effort: Some(OpenAiCompatibleReasoningEffort::Medium),
                summary: None,
            }),
            temperature: Some(0.7),
            top_p: Some(0.8),
            max_output_tokens: Some(256),
        }),
        Some(OpenAiOptions {
            metadata: std::collections::BTreeMap::from([(
                "trace_id".to_string(),
                "trace-1".to_string(),
            )]),
            service_tier: Some(OpenAiServiceTier::Priority),
            store: Some(true),
            prompt_cache_key: Some("cache-key-123".to_string()),
            prompt_cache_retention: Some(OpenAiPromptCacheRetention::TwentyFourHours),
            truncation: Some(OpenAiTruncation::Disabled),
            text: Some(OpenAiTextOptions {
                verbosity: Some(OpenAiTextVerbosity::Low),
            }),
            safety_identifier: Some("safe-id-123".to_string()),
            previous_response_id: Some("resp_abc123".to_string()),
            top_logprobs: Some(5),
            max_tool_calls: Some(2),
        }),
    ))
    .await
    .expect("request should succeed");

    let captured = captured.await.expect("captured request should arrive");
    assert_eq!(captured.path, "/v1/responses");
    assert_eq!(captured.body["model"], "gpt-5.1");
    assert_eq!(captured.body["parallel_tool_calls"], true);
    assert_eq!(captured.body["reasoning"], json!({ "effort": "medium" }));
    assert_json_number_close(&captured.body["temperature"], 0.7);
    assert_json_number_close(&captured.body["top_p"], 0.8);
    assert_eq!(captured.body["max_output_tokens"], 256);
    assert_eq!(captured.body["metadata"], json!({ "trace_id": "trace-1" }));
    assert_eq!(captured.body["service_tier"], "priority");
    assert_eq!(captured.body["store"], true);
    assert_eq!(captured.body["prompt_cache_key"], "cache-key-123");
    assert_eq!(captured.body["prompt_cache_retention"], "24h");
    assert_eq!(captured.body["truncation"], "disabled");
    assert_eq!(captured.body["text"]["verbosity"], "low");
    assert_eq!(captured.body["text"]["format"]["type"], "text");
    assert_eq!(captured.body["safety_identifier"], "safe-id-123");
    assert_eq!(captured.body["previous_response_id"], "resp_abc123");
    assert_eq!(captured.body["top_logprobs"], 5);
    assert_eq!(captured.body["max_tool_calls"], 2);
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
            reasoning: Some(OpenAiCompatibleReasoning {
                effort: Some(OpenAiCompatibleReasoningEffort::Low),
                summary: None,
            }),
            temperature: Some(0.65),
            top_p: Some(0.9),
            max_output_tokens: None,
        }),
        Some(OpenRouterOptions {
            fallback_models: vec!["openai/gpt-4.1-mini".to_string()],
            provider_preferences: Some(json!({ "order": ["openai"] })),
            plugins: vec![OpenRouterPlugin::Web(OpenRouterWebPlugin::default())],
            metadata: std::collections::BTreeMap::from([(
                "trace_id".to_string(),
                "trace-789".to_string(),
            )]),
            top_k: Some(12),
            top_logprobs: Some(5),
            max_tokens: Some(512),
            stop: vec!["DONE".to_string()],
            seed: Some(42),
            logit_bias: std::collections::BTreeMap::from([("42".to_string(), 10)]),
            logprobs: Some(true),
            frequency_penalty: Some(0.25),
            presence_penalty: Some(0.5),
            user: Some("user-123".to_string()),
            session_id: Some("session-456".to_string()),
            trace: Some(OpenRouterTrace {
                trace_id: Some("trace-789".to_string()),
                ..OpenRouterTrace::default()
            }),
            text: Some(OpenRouterTextOptions {
                verbosity: Some(OpenRouterTextVerbosity::Max),
            }),
            modalities: Some(vec!["text".to_string()]),
            image_config: Some(std::collections::BTreeMap::from([(
                "size".to_string(),
                OpenRouterImageConfigValue::String("1024x1024".to_string()),
            )])),
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
    assert_json_number_close(&captured.body["temperature"], 0.65);
    assert_json_number_close(&captured.body["top_p"], 0.9);
    assert_eq!(captured.body["provider"], json!({ "order": ["openai"] }));
    assert_eq!(captured.body["plugins"], json!([{ "id": "web" }]));
    assert_eq!(
        captured.body["metadata"],
        json!({ "trace_id": "trace-789" })
    );
    assert_eq!(captured.body["top_k"], 12);
    assert_eq!(captured.body["top_logprobs"], 5);
    assert_eq!(captured.body["max_tokens"], 512);
    assert_eq!(captured.body["stop"], json!(["DONE"]));
    assert_eq!(captured.body["seed"], 42);
    assert_eq!(captured.body["logit_bias"], json!({ "42": 10 }));
    assert_eq!(captured.body["logprobs"], true);
    assert_json_number_close(&captured.body["frequency_penalty"], 0.25);
    assert_json_number_close(&captured.body["presence_penalty"], 0.5);
    assert_eq!(captured.body["user"], "user-123");
    assert_eq!(captured.body["session_id"], "session-456");
    assert_eq!(captured.body["trace"], json!({ "trace_id": "trace-789" }));
    assert_eq!(captured.body["text"]["verbosity"], "max");
    assert_eq!(captured.body["modalities"], json!(["text"]));
    assert_eq!(
        captured.body["image_config"],
        json!({ "size": "1024x1024" })
    );
}

#[tokio::test]
async fn openai_direct_helper_omits_store_without_provider_option() {
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
        Some(OpenAiCompatibleOptions::default()),
        Some(OpenAiOptions::default()),
    ))
    .await
    .expect("request should succeed");

    let captured = captured.await.expect("captured request should arrive");
    assert!(captured.body.get("store").is_none());
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
            thinking: Some(AnthropicThinking::Enabled {
                budget_tokens:
                    AnthropicThinkingBudget::new(1024).expect("non-zero thinking budget"),
                display: None,
            }),
        }),
        Some(AnthropicOptions {
            temperature: Some(0.4),
            top_p: Some(0.6),
            max_tokens: Some(2048),
            top_k: Some(8),
            stop_sequences: vec!["DONE".to_string(), "STOP".to_string()],
            metadata_user_id: Some("anthropic-user-1".to_string()),
            output_config: Some(AnthropicOutputConfig {
                effort: Some(AnthropicOutputEffort::High),
                format: None,
            }),
            service_tier: Some(AnthropicServiceTier::StandardOnly),
            tool_choice: Some(AnthropicToolChoiceOptions::Auto {
                disable_parallel_tool_use: Some(false),
            }),
            inference_geo: Some("us".to_string()),
            cache_control: Some(AnthropicCacheControl {
                type_: AnthropicCacheControlType::Ephemeral,
                ttl: Some(AnthropicCacheControlTTL::FiveMinute),
            }),
            metadata: std::collections::BTreeMap::from([
                ("trace_id".to_string(), "trace-anthropic-1".to_string()),
                ("user_id".to_string(), "legacy-user".to_string()),
            ]),
        }),
    ))
    .await
    .expect("request should succeed");

    let captured = captured.await.expect("captured request should arrive");
    assert_eq!(captured.path, "/v1/messages");
    assert_eq!(captured.body["model"], "claude-opus-4-1-20250805");
    assert_eq!(
        captured.body["thinking"],
        json!({ "type": "enabled", "budget_tokens": 1024 })
    );
    assert_json_number_close(&captured.body["temperature"], 0.4);
    assert_json_number_close(&captured.body["top_p"], 0.6);
    assert_eq!(captured.body["max_tokens"], 2048);
    assert_eq!(captured.body["top_k"], 8);
    assert_eq!(captured.body["stop_sequences"], json!(["DONE", "STOP"]));
    assert_eq!(
        captured.body["metadata"],
        json!({ "trace_id": "trace-anthropic-1", "user_id": "anthropic-user-1" })
    );
    assert_eq!(captured.body["service_tier"], "standard_only");
    assert_eq!(captured.body["inference_geo"], json!("us"));
    assert_eq!(
        captured.body["cache_control"],
        json!({ "type": "ephemeral", "ttl": "5m" })
    );
    assert_eq!(
        captured.body.pointer("/output_config/effort"),
        Some(&json!("high"))
    );
    assert_eq!(
        captured
            .body
            .pointer("/tool_choice/disable_parallel_tool_use"),
        Some(&json!(false))
    );
}

#[tokio::test]
async fn openai_direct_helper_rejects_invalid_native_options_before_sending_request() {
    let (base_url, _captured) =
        spawn_json_capture_stub("application/json", OPENAI_SUCCESS_BODY).await;
    let client = openai()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("gpt-5-mini")
        .build()
        .expect("build openai client");

    let error = with_timeout(client.create_with_openai_options(
        MessageCreateInput::user("hello"),
        Some("gpt-5.1".to_string()),
        Some(OpenAiCompatibleOptions {
            temperature: Some(2.5),
            ..OpenAiCompatibleOptions::default()
        }),
        None,
    ))
    .await
    .expect_err("request should fail for invalid native options");

    assert!(error.to_string().contains("temperature"));
}

#[tokio::test]
async fn anthropic_direct_helper_rejects_invalid_native_options_before_sending_request() {
    let (base_url, _captured) =
        spawn_json_capture_stub("application/json", ANTHROPIC_SUCCESS_BODY).await;
    let client = anthropic()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("claude-sonnet-4-6")
        .build()
        .expect("build anthropic client");

    let error = with_timeout(client.create_with_anthropic_options(
        MessageCreateInput::user("hello"),
        Some("claude-opus-4-1-20250805".to_string()),
        None,
        Some(AnthropicOptions {
            metadata_user_id: Some("u".repeat(257)),
            ..AnthropicOptions::default()
        }),
    ))
    .await
    .expect_err("request should fail for invalid native options");

    assert!(error.to_string().contains("metadata.user_id"));
}

#[tokio::test]
async fn anthropic_direct_helper_rejects_invalid_thinking_budget_before_sending_request() {
    let (base_url, _captured) =
        spawn_json_capture_stub("application/json", ANTHROPIC_SUCCESS_BODY).await;
    let client = anthropic()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("claude-sonnet-4-6")
        .build()
        .expect("build anthropic client");

    let error = with_timeout(client.create_with_anthropic_options(
        MessageCreateInput::user("hello"),
        Some("claude-opus-4-1-20250805".to_string()),
        Some(AnthropicFamilyOptions {
            thinking: Some(AnthropicThinking::Enabled {
                budget_tokens: AnthropicThinkingBudget::new(512).expect("non-zero thinking budget"),
                display: None,
            }),
        }),
        Some(AnthropicOptions {
            max_tokens: Some(2048),
            ..AnthropicOptions::default()
        }),
    ))
    .await
    .expect_err("request should fail for invalid thinking budget");

    assert!(error.to_string().contains("thinking.budget_tokens"));
}

#[tokio::test]
async fn anthropic_direct_helper_rejects_thinking_budget_that_meets_or_exceeds_max_tokens() {
    let (base_url, _captured) =
        spawn_json_capture_stub("application/json", ANTHROPIC_SUCCESS_BODY).await;
    let client = anthropic()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("claude-sonnet-4-6")
        .build()
        .expect("build anthropic client");

    let error = with_timeout(client.create_with_anthropic_options(
        MessageCreateInput::user("hello"),
        Some("claude-opus-4-1-20250805".to_string()),
        Some(AnthropicFamilyOptions {
            thinking: Some(AnthropicThinking::Enabled {
                budget_tokens:
                    AnthropicThinkingBudget::new(2048).expect("non-zero thinking budget"),
                display: None,
            }),
        }),
        Some(AnthropicOptions {
            max_tokens: Some(2048),
            ..AnthropicOptions::default()
        }),
    ))
    .await
    .expect_err("request should fail when thinking budget reaches max_tokens");

    assert!(error.to_string().contains("less than max_tokens"));
}

#[tokio::test]
async fn openrouter_direct_helper_rejects_invalid_native_options_before_sending_request() {
    let (base_url, _captured) =
        spawn_json_capture_stub("application/json", OPENAI_SUCCESS_BODY).await;
    let client = openrouter()
        .api_key("test-key")
        .base_url(base_url)
        .default_model("openai/gpt-5-mini")
        .build()
        .expect("build openrouter client");

    let error = with_timeout(client.create_with_openrouter_options(
        MessageCreateInput::user("hello"),
        Some("openai/gpt-5.1".to_string()),
        None,
        Some(OpenRouterOptions {
            top_logprobs: Some(3),
            logprobs: Some(false),
            ..OpenRouterOptions::default()
        }),
    ))
    .await
    .expect_err("request should fail for invalid native options");

    assert!(error.to_string().contains("top_logprobs"));
}

#[test]
fn openrouter_native_options_do_not_include_removed_route_or_debug_fields() {
    let serialized =
        serde_json::to_value(OpenRouterOptions::default()).expect("options should serialize");
    let object = serialized
        .as_object()
        .expect("serialized options should be an object");

    assert!(
        object.get("route").is_none(),
        "route should not exist on typed OpenRouter options",
    );
    assert!(
        object.get("debug").is_none(),
        "debug should not exist on typed OpenRouter options",
    );
}
