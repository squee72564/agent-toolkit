#![cfg(feature = "live-tests")]

mod e2e;

use agent_toolkit::{
    ContentPart, Conversation, MessageCreateInput, ProviderKind, ToolChoice,
    core::CanonicalStreamEvent, openai,
};
use futures_util::StreamExt;

use e2e::live::{
    assert_live_response_meta, default_live_model, require_provider_api_key, response_text,
    with_live_test_timeout,
};
use e2e::tooling::{
    build_registry_with_raw_and_typed_tools, orchestrate_tool_calls, tool_enabled_input,
};

#[tokio::test]
async fn live_openai_envelope_stream_exposes_tool_call_deltas_and_final_tool_calls() {
    let Some(api_key) =
        require_provider_api_key(ProviderKind::OpenAi, "live OpenAI tool loop test")
    else {
        return;
    };

    let client = openai()
        .api_key(api_key)
        .default_model(default_live_model(ProviderKind::OpenAi))
        .build()
        .expect("build openai client");

    let registry = build_registry_with_raw_and_typed_tools();
    // This live assertion is intentionally scoped to OpenAI because the public Responses-style
    // tool streaming path is the most reliable place in this repo to expect a forced tool call.
    let prompt = "Call the get_weather tool for San Francisco, CA. After the tool result arrives, reply in one short sentence.";
    let input = tool_enabled_input(MessageCreateInput::user(prompt), &registry).with_tool_choice(
        ToolChoice::Specific {
            name: "get_weather".to_string(),
        },
    );

    let mut stream = with_live_test_timeout(client.streaming().create(input))
        .await
        .expect("stream should open");

    let mut saw_tool_delta = false;
    let mut saw_tool_item = false;
    while let Some(envelope) = stream.next().await {
        let envelope = envelope.expect("envelope should succeed");
        for event in envelope.canonical {
            match event {
                CanonicalStreamEvent::ToolCallArgumentsDelta { .. } => {
                    saw_tool_delta = true;
                }
                CanonicalStreamEvent::OutputItemStarted { item, .. } => {
                    if matches!(
                        item,
                        agent_toolkit::core::StreamOutputItemStart::ToolCall { .. }
                    ) {
                        saw_tool_item = true;
                    }
                }
                CanonicalStreamEvent::OutputItemCompleted { item, .. } => {
                    if matches!(
                        item,
                        agent_toolkit::core::StreamOutputItemEnd::ToolCall { .. }
                    ) {
                        saw_tool_item = true;
                    }
                }
                _ => {}
            }
        }
    }

    let completion = with_live_test_timeout(stream.finish())
        .await
        .expect("stream should finish successfully");
    assert_live_response_meta(&completion.meta, ProviderKind::OpenAi);

    let final_tool_calls: Vec<_> = completion
        .response
        .output
        .content
        .iter()
        .filter_map(|part| match part {
            ContentPart::ToolCall { tool_call } => Some(tool_call),
            _ => None,
        })
        .collect();

    assert!(
        saw_tool_delta || saw_tool_item || !final_tool_calls.is_empty(),
        "expected either live tool call deltas or at least one final tool call"
    );
    assert!(
        final_tool_calls
            .iter()
            .any(|tool_call| tool_call.name == "get_weather"),
        "expected the finalized response to expose a completed get_weather tool call"
    );

    let mut conversation = Conversation::new();
    conversation.push_user_text(prompt);
    let next_input = orchestrate_tool_calls(&completion.response, &mut conversation, &registry)
        .await
        .expect("tool orchestration should succeed")
        .expect("forced tool choice should produce a follow-up input");

    let follow_up = with_live_test_timeout(
        client
            .messages()
            .create(tool_enabled_input(next_input, &registry)),
    )
    .await
    .expect("follow-up tool loop request should succeed");

    assert!(
        !response_text(&follow_up.output.content).trim().is_empty(),
        "expected assistant follow-up after tool execution"
    );
}
