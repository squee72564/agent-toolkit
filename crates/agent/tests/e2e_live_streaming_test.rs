#![cfg(feature = "live-tests")]

mod e2e;

use agent_toolkit::{MessageCreateInput, ProviderId, anthropic, openai, openrouter};
use futures_util::StreamExt;

use e2e::live::provider_api_key;
use e2e::timeout::with_test_timeout;

#[tokio::test]
async fn live_openai_text_streaming_smoke_test() {
    let Some(api_key) = provider_api_key(ProviderId::OpenAi) else {
        eprintln!("skipping live OpenAI streaming test: OPENAI_API_KEY is not set");
        return;
    };

    let client = openai()
        .api_key(api_key)
        .default_model("gpt-5-mini")
        .build()
        .expect("build openai client");

    let mut stream = with_test_timeout(
        client
            .streaming()
            .create(MessageCreateInput::user("Reply with exactly: live stream ok")),
    )
    .await
    .expect("openai stream should open")
    .into_text_stream();

    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        output.push_str(&chunk.expect("text chunk should succeed"));
    }

    let completion = with_test_timeout(stream.finish())
        .await
        .expect("stream should finish successfully");

    assert!(
        !output.trim().is_empty(),
        "expected streamed output to contain assistant text"
    );
    assert!(
        !completion.response.output.content.is_empty(),
        "expected final response to contain synthesized output"
    );
}

#[tokio::test]
async fn live_anthropic_text_streaming_smoke_test() {
    let Some(api_key) = provider_api_key(ProviderId::Anthropic) else {
        eprintln!("skipping live Anthropic streaming test: ANTHROPIC_API_KEY is not set");
        return;
    };

    let client = anthropic()
        .api_key(api_key)
        .default_model("claude-sonnet-4-6")
        .build()
        .expect("build anthropic client");

    let mut stream = with_test_timeout(
        client
            .streaming()
            .create(MessageCreateInput::user("Reply with exactly: live stream ok")),
    )
    .await
    .expect("anthropic stream should open")
    .into_text_stream();

    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        output.push_str(&chunk.expect("text chunk should succeed"));
    }

    let completion = with_test_timeout(stream.finish())
        .await
        .expect("stream should finish successfully");

    assert!(
        !output.trim().is_empty(),
        "expected streamed output to contain assistant text"
    );
    assert!(
        !completion.response.output.content.is_empty(),
        "expected final response to contain synthesized output"
    );
}

#[tokio::test]
async fn live_openrouter_text_streaming_smoke_test() {
    let Some(api_key) = provider_api_key(ProviderId::OpenRouter) else {
        eprintln!("skipping live OpenRouter streaming test: OPENROUTER_API_KEY is not set");
        return;
    };

    let client = openrouter()
        .api_key(api_key)
        .default_model("openai/gpt-5-mini")
        .build()
        .expect("build openrouter client");

    let mut stream = with_test_timeout(
        client
            .streaming()
            .create(MessageCreateInput::user("Reply with exactly: live stream ok")),
    )
    .await
    .expect("openrouter stream should open")
    .into_text_stream();

    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        output.push_str(&chunk.expect("text chunk should succeed"));
    }

    let completion = with_test_timeout(stream.finish())
        .await
        .expect("stream should finish successfully");

    assert!(
        !output.trim().is_empty(),
        "expected streamed output to contain assistant text"
    );
    assert!(
        !completion.response.output.content.is_empty(),
        "expected final response to contain synthesized output"
    );
}
