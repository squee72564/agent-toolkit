#![cfg(all(
    feature = "live-tests",
    feature = "openai",
    feature = "anthropic",
    feature = "openrouter"
))]

mod e2e;

use agent_toolkit::core::ProviderKind;
use agent_toolkit::prelude::{MessageCreateInput, anthropic, openai, openrouter};
use futures_util::StreamExt;

use e2e::live::{
    assert_live_response_meta, default_live_model, require_provider_api_key, response_text,
    with_live_test_timeout,
};

async fn collect_text_stream_completion(
    mut stream: agent_toolkit::runtime::MessageTextStream,
    provider: ProviderKind,
) {
    let mut streamed_text = String::new();
    while let Some(chunk) = stream.next().await {
        streamed_text.push_str(&chunk.expect("text chunk should succeed"));
    }

    let completion = with_live_test_timeout(stream.finish())
        .await
        .expect("stream should finish successfully");

    let final_text = response_text(&completion.response.output.content);

    assert!(
        !streamed_text.trim().is_empty(),
        "expected streamed output to contain assistant text"
    );
    assert!(
        !final_text.trim().is_empty(),
        "expected finalized response to contain assistant text"
    );
    assert_live_response_meta(&completion.meta, provider);
}

#[tokio::test]
async fn live_openai_text_streaming_smoke_test() {
    let Some(api_key) =
        require_provider_api_key(ProviderKind::OpenAi, "live OpenAI streaming test")
    else {
        return;
    };

    let client = openai()
        .api_key(api_key)
        .default_model(default_live_model(ProviderKind::OpenAi))
        .build()
        .expect("build openai client");

    let stream = with_live_test_timeout(client.streaming().create(MessageCreateInput::user(
        "Reply with one short sentence confirming this live streaming smoke test.",
    )))
    .await
    .expect("openai stream should open")
    .into_text_stream();

    collect_text_stream_completion(stream, ProviderKind::OpenAi).await;
}

#[tokio::test]
async fn live_anthropic_text_streaming_smoke_test() {
    let Some(api_key) =
        require_provider_api_key(ProviderKind::Anthropic, "live Anthropic streaming test")
    else {
        return;
    };

    let client = anthropic()
        .api_key(api_key)
        .default_model(default_live_model(ProviderKind::Anthropic))
        .build()
        .expect("build anthropic client");

    let stream = with_live_test_timeout(client.create_stream_with_anthropic_options(
        MessageCreateInput::user(
            "Reply with one short sentence confirming this live streaming smoke test.",
        ),
        None,
        None,
        Some(agent_toolkit::core::AnthropicOptions {
            max_tokens: Some(128),
            ..agent_toolkit::core::AnthropicOptions::default()
        }),
    ))
    .await
    .expect("anthropic stream should open")
    .into_text_stream();

    collect_text_stream_completion(stream, ProviderKind::Anthropic).await;
}

#[tokio::test]
async fn live_openrouter_text_streaming_smoke_test() {
    let Some(api_key) =
        require_provider_api_key(ProviderKind::OpenRouter, "live OpenRouter streaming test")
    else {
        return;
    };

    let client = openrouter()
        .api_key(api_key)
        .default_model(default_live_model(ProviderKind::OpenRouter))
        .build()
        .expect("build openrouter client");

    let stream = with_live_test_timeout(client.streaming().create(MessageCreateInput::user(
        "Reply with one short sentence confirming this live streaming smoke test.",
    )))
    .await
    .expect("openrouter stream should open")
    .into_text_stream();

    collect_text_stream_completion(stream, ProviderKind::OpenRouter).await;
}
