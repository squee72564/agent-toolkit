mod e2e;

use std::time::Duration;

use agent_toolkit::tools::ToolRegistryError;
use agent_toolkit::{
    AssistantOutput, ContentPart, Conversation, FinishReason, MessageCreateInput, RuntimeErrorKind,
    ToolCall, Usage, anthropic, openai, openrouter,
};

use e2e::fixtures::{FixtureProvider, FixtureScenario, load_fixture_json};
use e2e::mock_server::{MockResponse, MockServer, unused_local_url};
use e2e::timeout::{with_test_timeout, with_timeout};
use e2e::tooling::{ToolLoopError, build_raw_echo_only_registry, orchestrate_tool_calls};

#[tokio::test]
async fn upstream_error_payload_decodes_to_runtime_upstream_kind_openai() {
    let body = load_fixture_json(
        FixtureProvider::OpenAi,
        FixtureScenario::InvalidAuth,
        "gpt-5-mini.json",
    );
    let server = MockServer::spawn(vec![MockResponse::json(401, body)]).await;
    let client = openai()
        .api_key("openai-key")
        .base_url(server.base_url())
        .default_model("gpt-5-mini")
        .build()
        .expect("build openai client");

    let error = with_test_timeout(
        client
            .messages()
            .create(MessageCreateInput::user("trigger openai")),
    )
    .await
    .expect_err("request should fail with upstream error");

    assert_eq!(error.kind, RuntimeErrorKind::Upstream);
}

#[tokio::test]
async fn upstream_error_payload_decodes_to_runtime_upstream_kind_anthropic() {
    let body = load_fixture_json(
        FixtureProvider::Anthropic,
        FixtureScenario::InvalidAuth,
        "claude-sonnet-4-5-20250929.json",
    );
    let server = MockServer::spawn(vec![MockResponse::json(401, body)]).await;
    let client = anthropic()
        .api_key("anthropic-key")
        .base_url(server.base_url())
        .default_model("claude-sonnet-4-5-20250929")
        .build()
        .expect("build anthropic client");

    let error = with_test_timeout(
        client
            .messages()
            .create(MessageCreateInput::user("trigger anthropic")),
    )
    .await
    .expect_err("request should fail with upstream error");

    assert_eq!(error.kind, RuntimeErrorKind::Upstream);
}

#[tokio::test]
async fn upstream_error_payload_decodes_to_runtime_upstream_kind_openrouter() {
    let body = load_fixture_json(
        FixtureProvider::OpenRouter,
        FixtureScenario::InvalidAuth,
        "openai.gpt-5-mini.json",
    );
    let server = MockServer::spawn(vec![MockResponse::json(401, body)]).await;
    let client = openrouter()
        .api_key("openrouter-key")
        .base_url(server.base_url())
        .default_model("openai.gpt-5-mini")
        .build()
        .expect("build openrouter client");

    let error = with_test_timeout(
        client
            .messages()
            .create(MessageCreateInput::user("trigger openrouter")),
    )
    .await
    .expect_err("request should fail with upstream error");

    assert_eq!(error.kind, RuntimeErrorKind::Upstream);
}

#[tokio::test]
async fn transport_failures_classify_as_runtime_transport() {
    let client = openai()
        .api_key("openai-key")
        .base_url(unused_local_url())
        .default_model("gpt-5-mini")
        .build()
        .expect("build openai client");

    let error = with_test_timeout(client.messages().create(MessageCreateInput::user("hello")))
        .await
        .expect_err("transport should fail");

    assert_eq!(error.kind, RuntimeErrorKind::Transport);
}

#[tokio::test]
async fn delayed_mock_responses_are_bounded_by_explicit_timeout_wrappers() {
    let success = load_fixture_json(
        FixtureProvider::OpenAi,
        FixtureScenario::BasicChat,
        "gpt-5-mini.json",
    );
    let server = MockServer::spawn(vec![
        MockResponse::json(200, success).with_delay(Duration::from_millis(400)),
    ])
    .await;

    let client = openai()
        .api_key("openai-key")
        .base_url(server.base_url())
        .default_model("gpt-5-mini")
        .build()
        .expect("build openai client");

    let timeout_result = tokio::time::timeout(
        Duration::from_millis(50),
        client.messages().create(MessageCreateInput::user("hello")),
    )
    .await;

    assert!(
        timeout_result.is_err(),
        "expected explicit timeout wrapper to fire"
    );

    let _ = with_timeout(
        Duration::from_secs(2),
        client.messages().create(MessageCreateInput::user("hello")),
    )
    .await;
}

#[tokio::test]
async fn orchestration_loop_reports_invalid_tool_args_predictably() {
    let response = agent_toolkit::Response {
        output: AssistantOutput {
            content: vec![ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: "call_1".to_string(),
                    name: "raw_echo".to_string(),
                    arguments_json: serde_json::json!({}),
                },
            }],
            structured_output: None,
        },
        usage: Usage::default(),
        model: "test-model".to_string(),
        raw_provider_response: None,
        finish_reason: FinishReason::ToolCalls,
        warnings: Vec::new(),
    };

    let registry = build_raw_echo_only_registry();
    let mut conversation = Conversation::new();
    conversation.push_user_text("Run a tool.");

    let error = orchestrate_tool_calls(&response, &mut conversation, &registry)
        .await
        .expect_err("invalid args should bubble as tool loop error");

    match error {
        ToolLoopError::Registry(registry_error) => {
            assert!(matches!(
                registry_error,
                ToolRegistryError::InvalidArgs { .. }
            ));
        }
    }
}
