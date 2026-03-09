mod e2e;

use std::time::Duration;

use agent_toolkit::{
    Conversation, MessageCreateInput, ProviderId, ToolChoice, anthropic, openai, openrouter,
};

use e2e::assertions::{
    assert_auth_api_key, assert_auth_bearer, assert_header, assert_json_array_len_at_least,
    assert_json_object_has_key, assert_json_string, assert_post_path,
};
use e2e::fixtures::{FixtureProvider, FixtureScenario, load_fixture_json};
use e2e::mock_server::{MockResponse, MockServer};
use e2e::timeout::with_test_timeout;
use e2e::tooling::{
    build_registry_with_raw_and_typed_tools, execute_raw_echo, execute_typed_echo,
    orchestrate_tool_calls, tool_enabled_input,
};

#[tokio::test]
async fn openai_messages_create_happy_path_uses_fixture_and_expected_request_shape() {
    let response_fixture = load_fixture_json(
        FixtureProvider::OpenAi,
        FixtureScenario::BasicChat,
        "gpt-5-mini.json",
    );

    let server = MockServer::spawn(vec![
        MockResponse::json(200, response_fixture).with_header("x-request-id", "req_openai_1"),
    ])
    .await;

    let client = openai()
        .api_key("test-openai-key")
        .base_url(server.base_url())
        .default_model("gpt-5-mini")
        .build()
        .expect("build openai client");

    let (_response, meta) = with_test_timeout(
        client
            .messages()
            .create_with_meta(MessageCreateInput::user("hello openai")),
    )
    .await
    .expect("openai request should succeed");

    assert_eq!(meta.selected_provider, ProviderId::OpenAi);
    assert_eq!(meta.selected_model, "gpt-5-mini");
    assert_eq!(meta.status_code, Some(200));
    assert_eq!(meta.request_id.as_deref(), Some("req_openai_1"));
    assert_eq!(meta.attempts.len(), 1);

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];

    assert_post_path(request, "/v1/responses");
    assert_auth_bearer(request, "test-openai-key");
    assert_json_string(&request.body_json, "/model", "gpt-5-mini");
    assert_json_array_len_at_least(&request.body_json, "/input", 1);
}

#[tokio::test]
async fn anthropic_conversation_flow_preserves_history_between_turns() {
    let first_response = load_fixture_json(
        FixtureProvider::Anthropic,
        FixtureScenario::BasicChat,
        "claude-sonnet-4-6.json",
    );
    let second_response = load_fixture_json(
        FixtureProvider::Anthropic,
        FixtureScenario::BasicChat,
        "claude-sonnet-4-6.json",
    );

    let server = MockServer::spawn(vec![
        MockResponse::json(200, first_response).with_header("request-id", "anthropic_req_1"),
        MockResponse::json(200, second_response).with_header("request-id", "anthropic_req_2"),
    ])
    .await;

    let client = anthropic()
        .api_key("test-anthropic-key")
        .base_url(server.base_url())
        .default_model("claude-sonnet-4-6")
        .build()
        .expect("build anthropic client");

    let mut conversation = Conversation::new();
    conversation.push_user_text("hello anthropic");

    let _ = with_test_timeout(client.messages().create(conversation.to_input()))
        .await
        .expect("first anthropic turn succeeds");

    conversation.push_assistant_text("ack");
    conversation.push_user_text("follow up question");

    let _ = with_test_timeout(client.messages().create(conversation.to_input()))
        .await
        .expect("second anthropic turn succeeds");

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 2);

    let first = &requests[0];
    let second = &requests[1];

    assert_post_path(first, "/v1/messages");
    assert_auth_api_key(first, "test-anthropic-key");
    assert_header(first, "anthropic-version", "2023-06-01");
    assert_json_string(&first.body_json, "/model", "claude-sonnet-4-6");

    let first_message_count = first
        .body_json
        .pointer("/messages")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .expect("first request has messages array");
    let second_message_count = second
        .body_json
        .pointer("/messages")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .expect("second request has messages array");
    assert!(second_message_count > first_message_count);
}

#[tokio::test]
async fn openrouter_tool_enabled_flow_handles_tool_orchestration_and_meta() {
    let tool_call_response = load_fixture_json(
        FixtureProvider::OpenRouter,
        FixtureScenario::ToolCall,
        "openai.gpt-5.4.json",
    );
    let final_response = load_fixture_json(
        FixtureProvider::OpenRouter,
        FixtureScenario::BasicChat,
        "openai.gpt-5.4.json",
    );

    let server = MockServer::spawn(vec![
        MockResponse::json(200, tool_call_response).with_header("x-request-id", "or_req_1"),
        MockResponse::json(200, final_response).with_header("x-request-id", "or_req_2"),
    ])
    .await;

    let client = openrouter()
        .api_key("test-openrouter-key")
        .base_url(server.base_url())
        .default_model("openai.gpt-5.4")
        .request_timeout(Duration::from_secs(2))
        .stream_timeout(Duration::from_secs(2))
        .build()
        .expect("build openrouter client");

    let registry = build_registry_with_raw_and_typed_tools();

    let mut conversation = Conversation::new();
    conversation.push_user_text("use a tool");

    let (first_response, first_meta) = with_test_timeout(client.messages().create_with_meta(
        tool_enabled_input(conversation.to_input(), &registry).with_tool_choice(ToolChoice::Auto),
    ))
    .await
    .expect("first openrouter request succeeds");

    assert_eq!(first_meta.selected_provider, ProviderId::OpenRouter);
    assert_eq!(first_meta.selected_model, "openai.gpt-5.4");

    let next_input = orchestrate_tool_calls(&first_response, &mut conversation, &registry)
        .await
        .expect("tool orchestration should succeed")
        .expect("tool call fixture should produce follow-up input");

    let (_second_response, second_meta) = with_test_timeout(client.messages().create_with_meta(
        tool_enabled_input(next_input, &registry).with_tool_choice(ToolChoice::Auto),
    ))
    .await
    .expect("second openrouter request succeeds");

    assert_eq!(second_meta.selected_provider, ProviderId::OpenRouter);
    assert_eq!(second_meta.status_code, Some(200));
    assert_eq!(second_meta.request_id.as_deref(), Some("or_req_2"));

    let raw_output = execute_raw_echo(&registry, serde_json::json!({ "value": "raw" }))
        .await
        .expect("raw echo executes");
    assert_eq!(raw_output.content, serde_json::json!({ "value": "raw" }));

    let typed_output = execute_typed_echo(&registry, serde_json::json!({ "value": "typed" }))
        .await
        .expect("typed echo executes");
    assert_eq!(
        typed_output.content,
        serde_json::json!({ "wrapped": "typed:typed" })
    );

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 2);

    let request = &requests[0];
    assert_post_path(request, "/v1/responses");
    assert_auth_bearer(request, "test-openrouter-key");
    assert_json_string(&request.body_json, "/model", "openai.gpt-5.4");
    assert_json_object_has_key(&request.body_json, "/tools/0", "name");
    assert_json_object_has_key(&request.body_json, "/tools/0", "parameters");
}
