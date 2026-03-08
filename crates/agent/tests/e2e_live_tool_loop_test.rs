#![cfg(feature = "live-tests")]

mod e2e;

use agent_toolkit::{ContentPart, MessageCreateInput, ProviderId, ToolChoice, openai};
use futures_util::StreamExt;

use e2e::live::provider_api_key;
use e2e::timeout::with_test_timeout;
use e2e::tooling::{build_registry_with_raw_and_typed_tools, orchestrate_tool_calls, tool_enabled_input};

#[tokio::test]
async fn live_openai_envelope_stream_exposes_tool_call_deltas_and_final_tool_calls() {
    let Some(api_key) = provider_api_key(ProviderId::OpenAi) else {
        eprintln!("skipping live OpenAI tool loop test: OPENAI_API_KEY is not set");
        return;
    };

    let client = openai()
        .api_key(api_key)
        .default_model("gpt-5-mini")
        .build()
        .expect("build openai client");

    let registry = build_registry_with_raw_and_typed_tools();
    let input = tool_enabled_input(
        MessageCreateInput::user("Use get_weather for San Francisco, then summarize briefly."),
        &registry,
    )
    .with_tool_choice(ToolChoice::Auto);

    let mut stream = with_test_timeout(client.streaming().create(input))
        .await
        .expect("stream should open");

    let mut saw_tool_delta = false;
    while let Some(envelope) = stream.next().await {
        let envelope = envelope.expect("envelope should succeed");
        if envelope
            .canonical
            .iter()
            .any(|event| matches!(event, agent_toolkit::core::CanonicalStreamEvent::ToolCallArgumentsDelta { .. }))
        {
            saw_tool_delta = true;
        }
    }

    let completion = with_test_timeout(stream.finish())
        .await
        .expect("stream should finish successfully");

    let tool_call_count = completion
        .response
        .output
        .content
        .iter()
        .filter(|part| matches!(part, ContentPart::ToolCall { .. }))
        .count();

    assert!(
        saw_tool_delta || tool_call_count > 0,
        "expected either live tool call deltas or at least one final tool call"
    );

    let mut conversation = agent_toolkit::Conversation::new();
    conversation.push_user_text("Use get_weather for San Francisco, then summarize briefly.");
    let next_input = orchestrate_tool_calls(&completion.response, &mut conversation, &registry)
        .await
        .expect("tool orchestration should succeed");

    if let Some(next_input) = next_input {
        let follow_up = with_test_timeout(client.messages().create(tool_enabled_input(next_input, &registry)))
            .await
            .expect("follow-up tool loop request should succeed");
        assert!(
            !follow_up.output.content.is_empty(),
            "expected assistant follow-up after tool execution"
        );
    }
}
