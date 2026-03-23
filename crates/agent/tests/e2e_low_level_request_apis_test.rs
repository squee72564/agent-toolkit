#![cfg(all(feature = "openai", feature = "anthropic", feature = "openrouter"))]

mod e2e;

use agent_toolkit::core::{MessageRole, ProviderInstanceId, ProviderKind};
use agent_toolkit::prelude::{MessageCreateInput, Route, Target, ToolChoice, anthropic, openai};
use agent_toolkit::runtime::{ExecutionOptions, ProviderConfig};
use agent_toolkit::{AgentToolkit, ContentPart, Message, TaskRequest, ToolDefinition};

use e2e::assertions::{
    assert_auth_api_key, assert_auth_bearer, assert_header, assert_json_object_has_key,
    assert_json_string, assert_post_path,
};
use e2e::fixtures::{FixtureProvider, FixtureScenario, load_fixture_json};
use e2e::mock_server::{MockResponse, MockServer};
use e2e::timeout::with_test_timeout;

fn explicit_task_with_text(text: &str) -> TaskRequest {
    MessageCreateInput::new(vec![Message::new(
        MessageRole::User,
        vec![ContentPart::text(text)],
    )])
    .with_tools(vec![ToolDefinition {
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
    }])
    .with_tool_choice(ToolChoice::Specific {
        name: "raw_echo".to_string(),
    })
    .with_response_format(Default::default())
    .into_task_request()
    .expect("explicit task should build")
}

fn explicit_task() -> agent_toolkit::TaskRequest {
    explicit_task_with_text("hello from explicit task")
}

#[tokio::test]
async fn create_task_with_meta_openai_uses_explicit_task_and_captures_shape() {
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

    let task = explicit_task_with_text("hello from explicit request");
    let (_response, meta) = with_test_timeout(
        client
            .messages()
            .create_task_with_meta(task, ExecutionOptions::default()),
    )
    .await
    .expect("create_task_with_meta should succeed");

    assert_eq!(meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(meta.selected_model, "ignored-default");

    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 1);
    let req = &captured[0];

    assert_post_path(req, "/v1/responses");
    assert_auth_bearer(req, "openai-key");
    assert_json_string(&req.body_json, "/model", "ignored-default");
    assert_json_object_has_key(&req.body_json, "/tools/0", "name");
    assert_json_object_has_key(&req.body_json, "/tools/0", "parameters");
}

#[tokio::test]
async fn create_task_with_meta_anthropic_uses_expected_auth_headers_and_tool_choice_mapping() {
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

    let task = explicit_task_with_text("hello from explicit request");
    let _ = with_test_timeout(
        client
            .messages()
            .create_task_with_meta(task, ExecutionOptions::default()),
    )
    .await
    .expect("anthropic task should succeed");

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
async fn toolkit_execute_with_meta_honors_target_model_and_execution_headers() {
    let fixture = load_fixture_json(
        FixtureProvider::OpenRouter,
        FixtureScenario::BasicChat,
        "openai.gpt-5.4.json",
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

    let task = explicit_task_with_text("hello from explicit request");
    let route = Route::to(
        Target::new(ProviderInstanceId::openrouter_default()).with_model("openai.gpt-5.4"),
    );
    let mut execution = ExecutionOptions::default();
    execution
        .transport
        .extra_headers
        .insert("x-e2e-meta".to_string(), "header-value".to_string());

    let (_response, meta) = with_test_timeout(toolkit.execute_with_meta(task, route, execution))
        .await
        .expect("toolkit execute_with_meta should succeed");

    assert_eq!(meta.selected_provider_kind, ProviderKind::OpenRouter);
    assert_eq!(meta.selected_model, "openai.gpt-5.4");

    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 1);

    let req = &captured[0];
    assert_post_path(req, "/v1/responses");
    assert_auth_bearer(req, "openrouter-key");
    assert_header(req, "x-e2e-meta", "header-value");
    assert_json_string(&req.body_json, "/model", "openai.gpt-5.4");
}

#[tokio::test]
async fn toolkit_execute_with_meta_honors_route_and_execution_headers() {
    let fixture = load_fixture_json(
        FixtureProvider::OpenRouter,
        FixtureScenario::BasicChat,
        "openai.gpt-5.4.json",
    );
    let server = MockServer::spawn(vec![
        MockResponse::json(200, fixture).with_header("x-request-id", "low_openrouter_task_1"),
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

    let task = explicit_task();
    let route = Route::to(
        Target::new(ProviderInstanceId::openrouter_default()).with_model("openai.gpt-5.4"),
    );
    let mut execution = ExecutionOptions::default();
    execution
        .transport
        .extra_headers
        .insert("x-e2e-meta".to_string(), "header-value".to_string());

    let (_response, meta) = with_test_timeout(toolkit.execute_with_meta(task, route, execution))
        .await
        .expect("toolkit execute_with_meta should succeed");

    assert_eq!(meta.selected_provider_kind, ProviderKind::OpenRouter);
    assert_eq!(meta.selected_model, "openai.gpt-5.4");

    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 1);

    let req = &captured[0];
    assert_post_path(req, "/v1/responses");
    assert_auth_bearer(req, "openrouter-key");
    assert_header(req, "x-e2e-meta", "header-value");
    assert_json_string(&req.body_json, "/model", "openai.gpt-5.4");
}

#[tokio::test]
async fn router_messages_create_task_and_execute_match_same_explicit_contract() {
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

    let route =
        Route::to(Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5-mini"));
    let task_a = explicit_task_with_text("hello from explicit request");
    let task_b = explicit_task_with_text("hello from explicit request");

    let _ = with_test_timeout(toolkit.messages().create_task_with_meta(
        task_a,
        route.clone(),
        ExecutionOptions::default(),
    ))
    .await
    .expect("create_task_with_meta succeeds");

    let _ =
        with_test_timeout(toolkit.execute_with_meta(task_b, route, ExecutionOptions::default()))
            .await
            .expect("execute_with_meta succeeds");

    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 2);

    for req in captured {
        assert_post_path(&req, "/v1/responses");
        assert_auth_bearer(&req, "openai-key");
        assert_json_string(&req.body_json, "/model", "gpt-5-mini");
    }

    let _ = MessageCreateInput::user("coverage keepalive");
}
