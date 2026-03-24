#![cfg(all(feature = "live-tests", feature = "anthropic"))]

mod e2e;

use agent_toolkit::core::{
    AnthropicFamilyOptions, AnthropicOptions, AnthropicOutputConfig, AnthropicOutputEffort,
    AnthropicThinking, AnthropicThinkingBudget, AnthropicThinkingDisplay, FinishReason,
    ProviderKind, Response,
};
use agent_toolkit::prelude::{MessageCreateInput, anthropic};

use e2e::live::{require_provider_api_key, response_text, with_live_test_timeout};

const HAIKU_EXTENDED_THINKING_MODEL: &str = "claude-haiku-4-5-20251001";
const SONNET_ADAPTIVE_THINKING_MODEL: &str = "claude-sonnet-4-6";

fn assert_usage_recorded(response: &Response) {
    assert!(
        response.usage.derived_total_tokens() > 0,
        "expected provider response to report token usage"
    );
}

#[tokio::test]
async fn live_anthropic_extended_thinking_smoke_test() {
    let Some(api_key) = require_provider_api_key(
        ProviderKind::Anthropic,
        "live Anthropic extended-thinking smoke test",
    ) else {
        return;
    };

    let client = anthropic()
        .api_key(api_key)
        .default_model(HAIKU_EXTENDED_THINKING_MODEL)
        .build()
        .expect("build anthropic client");

    let response = with_live_test_timeout(
        client.create_with_anthropic_options(
            MessageCreateInput::user("Reply with exactly OK."),
            None,
            Some(AnthropicFamilyOptions {
                thinking: Some(AnthropicThinking::Enabled {
                    budget_tokens: AnthropicThinkingBudget::new(1024)
                        .expect("minimum thinking budget should be non-zero"),
                    display: Some(AnthropicThinkingDisplay::Omitted),
                }),
            }),
            Some(AnthropicOptions {
                max_tokens: Some(1056),
                ..AnthropicOptions::default()
            }),
        ),
    )
    .await
    .expect("anthropic extended-thinking request should succeed");

    assert!(
        !response_text(&response.output.content).trim().is_empty(),
        "expected finalized response content"
    );
    assert_usage_recorded(&response);
    assert_ne!(
        response.finish_reason,
        FinishReason::Error,
        "expected Anthropic live request to complete successfully"
    );
}

#[tokio::test]
async fn live_anthropic_adaptive_thinking_smoke_test() {
    let Some(api_key) = require_provider_api_key(
        ProviderKind::Anthropic,
        "live Anthropic adaptive-thinking smoke test",
    ) else {
        return;
    };

    let client = anthropic()
        .api_key(api_key)
        .default_model(SONNET_ADAPTIVE_THINKING_MODEL)
        .build()
        .expect("build anthropic client");

    let response = with_live_test_timeout(client.create_with_anthropic_options(
        MessageCreateInput::user("Reply with exactly OK."),
        None,
        Some(AnthropicFamilyOptions {
            thinking: Some(AnthropicThinking::Adaptive {
                display: Some(AnthropicThinkingDisplay::Omitted),
            }),
        }),
        Some(AnthropicOptions {
            max_tokens: Some(128),
            output_config: Some(AnthropicOutputConfig {
                effort: Some(AnthropicOutputEffort::Low),
                format: None,
            }),
            ..AnthropicOptions::default()
        }),
    ))
    .await
    .expect("anthropic adaptive-thinking request should succeed");

    assert!(
        !response_text(&response.output.content).trim().is_empty(),
        "expected finalized response content"
    );
    assert_usage_recorded(&response);
    assert_ne!(
        response.finish_reason,
        FinishReason::Error,
        "expected Anthropic live request to complete successfully"
    );
}
