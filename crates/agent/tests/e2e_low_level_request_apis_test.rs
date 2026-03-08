mod e2e;

use std::collections::BTreeMap;

use agent_toolkit::{
    AgentToolkit, ContentPart, Message, MessageCreateInput, MessageRole, ProviderConfig,
    ProviderId, Request, SendOptions, Target, ToolChoice, ToolDefinition, anthropic, openai,
};

use e2e::assertions::{
    assert_auth_api_key, assert_auth_bearer, assert_header, assert_json_object_has_key,
    assert_json_string, assert_post_path,
};
use e2e::fixtures::{FixtureProvider, FixtureScenario, load_fixture_json};
use e2e::mock_server::{MockResponse, MockServer};
use e2e::timeout::with_test_timeout;

fn explicit_request(model_id: &str) -> Request {
    let mut metadata = BTreeMap::new();
    metadata.insert("trace_id".to_string(), "trace-123".to_string());

    Request {
        model_id: model_id.to_string(),
        stream: false,
        messages: vec![Message::new(
            MessageRole::User,
            vec![ContentPart::text("hello from explicit request")],
        )],
        tools: vec![ToolDefinition {
            name: "raw_echo".to_string(),
            description: Some("echo tool".to_string()),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                },
                "required": ["value"],
                "additionalProperties": false
            }),
        }],
        tool_choice: ToolChoice::Specific {
            name: "raw_echo".to_string(),
        },
        response_format: Default::default(),
        temperature: Some(0.2),
        top_p: None,
        max_output_tokens: Some(128),
        stop: vec![],
        metadata,
    }
}

#[tokio::test]
async fn create_request_with_meta_openai_uses_explicit_request_and_captures_shape() {
    let fixture = load_fixture_json(
        FixtureProvider::OpenAi,
        FixtureScenario::BasicChat,
        "gpt-5-mini.json",
    );
    let server = MockServer::spawn(vec![
        MockResponse::json(200, fixture).with_header("x-request-id", "low_openai_1"),
    ])
    .await;

    let client = openai()
        .api_key("openai-key")
        .base_url(server.base_url())
        .default_model("ignored-default")
        .build()
        .expect("build openai client");

    let request = explicit_request("gpt-5-mini");
    let (_response, meta) =
        with_test_timeout(client.messages().create_request_with_meta(request.clone()))
            .await
            .expect("create_request_with_meta should succeed");

    assert_eq!(meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(meta.selected_model, "gpt-5-mini");

    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 1);
    let req = &captured[0];

    assert_post_path(req, "/v1/responses");
    assert_auth_bearer(req, "openai-key");
    assert_json_string(&req.body_json, "/model", "gpt-5-mini");
    assert_json_object_has_key(&req.body_json, "/tools/0", "name");
    assert_json_object_has_key(&req.body_json, "/tools/0", "parameters");
    assert_json_object_has_key(&req.body_json, "/metadata", "trace_id");
}

#[tokio::test]
async fn create_request_with_meta_anthropic_uses_expected_auth_headers_and_tool_choice_mapping() {
    let fixture = load_fixture_json(
        FixtureProvider::Anthropic,
        FixtureScenario::BasicChat,
        "claude-sonnet-4-6.json",
    );
    let server = MockServer::spawn(vec![
        MockResponse::json(200, fixture).with_header("request-id", "low_anthropic_1"),
    ])
    .await;

    let client = anthropic()
        .api_key("anthropic-key")
        .base_url(server.base_url())
        .default_model("claude-sonnet-4-6")
        .build()
        .expect("build anthropic client");

    let request = explicit_request("claude-sonnet-4-6");
    let _ = with_test_timeout(client.messages().create_request_with_meta(request))
        .await
        .expect("anthropic request should succeed");

    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 1);
    let req = &captured[0];

    assert_post_path(req, "/v1/messages");
    assert_auth_api_key(req, "anthropic-key");
    assert_header(req, "anthropic-version", "2023-06-01");
    assert_json_string(&req.body_json, "/model", "claude-sonnet-4-6");
    assert_json_object_has_key(&req.body_json, "/tools/0", "name");
    assert_json_object_has_key(&req.body_json, "/tool_choice", "type");
}

#[tokio::test]
async fn toolkit_send_with_meta_honors_target_model_and_send_metadata_headers() {
    let fixture = load_fixture_json(
        FixtureProvider::OpenRouter,
        FixtureScenario::BasicChat,
        "openai.gpt-5-mini.json",
    );
    let server = MockServer::spawn(vec![
        MockResponse::json(200, fixture).with_header("x-request-id", "low_openrouter_1"),
    ])
    .await;

    let toolkit = AgentToolkit::builder()
        .with_openrouter(
            ProviderConfig::new("openrouter-key")
                .with_base_url(server.base_url())
                .with_default_model("openai.gpt-5-nano"),
        )
        .build()
        .expect("build toolkit");

    let request = explicit_request("openai.gpt-5-nano");

    let mut options = SendOptions::for_target(
        Target::new(ProviderId::OpenRouter).with_model("openai.gpt-5-mini"),
    );
    options.metadata.insert(
        "transport.header.x-e2e-meta".to_string(),
        "header-value".to_string(),
    );

    let (_response, meta) = with_test_timeout(toolkit.send_with_meta(request, options))
        .await
        .expect("toolkit send_with_meta should succeed");

    assert_eq!(meta.selected_provider, ProviderId::OpenRouter);
    assert_eq!(meta.selected_model, "openai.gpt-5-mini");

    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 1);

    let req = &captured[0];
    assert_post_path(req, "/v1/chat/completions");
    assert_auth_bearer(req, "openrouter-key");
    assert_header(req, "x-e2e-meta", "header-value");
    assert_json_string(&req.body_json, "/model", "openai.gpt-5-mini");
}

#[tokio::test]
async fn router_messages_create_request_and_send_match_same_explicit_contract() {
    let fixture = load_fixture_json(
        FixtureProvider::OpenAi,
        FixtureScenario::BasicChat,
        "gpt-5-mini.json",
    );

    let server = MockServer::spawn(vec![
        MockResponse::json(200, fixture.clone()),
        MockResponse::json(200, fixture),
    ])
    .await;

    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("openai-key")
                .with_base_url(server.base_url())
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("build toolkit");

    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi).with_model("gpt-5-mini"));

    let request_a = explicit_request("gpt-5-mini");
    let request_b = explicit_request("gpt-5-mini");

    let _ = with_test_timeout(
        toolkit
            .messages()
            .create_request_with_meta(request_a, options.clone()),
    )
    .await
    .expect("create_request_with_meta succeeds");

    let _ = with_test_timeout(toolkit.send_with_meta(request_b, options))
        .await
        .expect("send_with_meta succeeds");

    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 2);

    for req in captured {
        assert_post_path(&req, "/v1/responses");
        assert_auth_bearer(&req, "openai-key");
        assert_json_string(&req.body_json, "/model", "gpt-5-mini");
    }

    let _ = MessageCreateInput::user("coverage keepalive");
}
