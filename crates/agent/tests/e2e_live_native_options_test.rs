#![cfg(all(
    feature = "live-tests",
    feature = "openai",
    feature = "anthropic",
    feature = "openrouter"
))]

mod e2e;

use std::collections::BTreeMap;

use agent_toolkit::core::{
    AnthropicOptions, FinishReason, OpenAiCompatibleOptions, OpenAiOptions, OpenAiTextOptions,
    OpenAiTextVerbosity, OpenRouterOptions, Response,
};
use agent_toolkit::prelude::{MessageCreateInput, anthropic, openai, openrouter};
use serde_json::Value;

use agent_toolkit::core::ProviderKind;
use e2e::live::{
    default_live_model, require_provider_api_key, response_text, with_live_test_timeout,
};

fn live_native_options_model(provider: ProviderKind) -> &'static str {
    match provider {
        // `gpt-5-mini` rejects `temperature` on the live Responses API. Use a
        // cheap model that still accepts the tuning knobs exercised here.
        ProviderKind::OpenAi => "gpt-4.1-mini",
        ProviderKind::OpenRouter => "openai/gpt-4.1-mini",
        _ => default_live_model(provider),
    }
}

fn assert_usage_recorded(response: &Response) {
    assert!(
        response.usage.derived_total_tokens() > 0,
        "expected provider response to report token usage"
    );
}

fn assert_json_number_close(actual: &Value, expected: f64) {
    let actual = actual
        .as_f64()
        .expect("expected JSON number in raw provider response");
    let delta = (actual - expected).abs();
    assert!(
        delta < 1e-6,
        "expected numeric value close to {expected}, got {actual} (delta {delta})"
    );
}

#[tokio::test]
async fn live_openai_direct_native_options_smoke_test() {
    let Some(api_key) =
        require_provider_api_key(ProviderKind::OpenAi, "live OpenAI native-options test")
    else {
        return;
    };

    let client = openai()
        .api_key(api_key)
        .default_model(live_native_options_model(ProviderKind::OpenAi))
        .build()
        .expect("build openai client");

    let response = with_live_test_timeout(client.create_with_openai_options(
        MessageCreateInput::user(
            "Reply with one short sentence confirming the OpenAI live native-options smoke test.",
        ),
        None,
        Some(OpenAiCompatibleOptions {
            temperature: Some(0.7),
            max_output_tokens: Some(96),
            ..OpenAiCompatibleOptions::default()
        }),
        Some(OpenAiOptions {
            metadata: BTreeMap::from([(
                "trace_id".to_string(),
                "live-openai-native-options".to_string(),
            )]),
            store: Some(false),
            text: Some(OpenAiTextOptions {
                verbosity: Some(OpenAiTextVerbosity::Medium),
            }),
            ..OpenAiOptions::default()
        }),
    ))
    .await
    .expect("openai native-options request should succeed");

    assert!(
        !response_text(&response.output.content).trim().is_empty(),
        "expected finalized response content"
    );
    assert_usage_recorded(&response);

    if let Some(raw) = response.raw_provider_response.as_ref() {
        assert_json_number_close(&raw["temperature"], 0.7);
        assert_eq!(raw["max_output_tokens"], 96);
        assert_eq!(raw["metadata"]["trace_id"], "live-openai-native-options");
        assert_eq!(raw["text"]["verbosity"], "medium");
        assert_eq!(raw["store"], false);
    }
}

#[tokio::test]
async fn live_anthropic_direct_native_options_smoke_test() {
    let Some(api_key) = require_provider_api_key(
        ProviderKind::Anthropic,
        "live Anthropic native-options test",
    ) else {
        return;
    };

    let client = anthropic()
        .api_key(api_key)
        .default_model(default_live_model(ProviderKind::Anthropic))
        .build()
        .expect("build anthropic client");

    let response = with_live_test_timeout(client.create_with_anthropic_options(
        MessageCreateInput::user(
            "Reply with one short sentence confirming the Anthropic live native-options smoke test.",
        ),
        None,
        None,
        Some(AnthropicOptions {
            temperature: Some(0.4),
            max_tokens: Some(96),
            top_k: Some(8),
            stop_sequences: vec!["__never_hit__".to_string()],
            metadata_user_id: Some("live-anthropic-smoke".to_string()),
            ..AnthropicOptions::default()
        }),
    ))
    .await
    .expect("anthropic native-options request should succeed");

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
async fn live_openrouter_direct_native_options_smoke_test() {
    let Some(api_key) = require_provider_api_key(
        ProviderKind::OpenRouter,
        "live OpenRouter native-options test",
    ) else {
        return;
    };

    let client = openrouter()
        .api_key(api_key)
        .default_model(live_native_options_model(ProviderKind::OpenRouter))
        .build()
        .expect("build openrouter client");

    let response = with_live_test_timeout(client.create_with_openrouter_options(
        MessageCreateInput::user(
            "Reply with one short sentence confirming the OpenRouter live native-options smoke test.",
        ),
        None,
        Some(OpenAiCompatibleOptions {
            temperature: Some(0.65),
            max_output_tokens: Some(96),
            ..OpenAiCompatibleOptions::default()
        }),
        Some(OpenRouterOptions {
            metadata: BTreeMap::from([(
                "trace_id".to_string(),
                "live-openrouter-native-options".to_string(),
            )]),
            top_k: Some(12),
            user: Some("live-openrouter-smoke".to_string()),
            session_id: Some("live-openrouter-session".to_string()),
            ..OpenRouterOptions::default()
        }),
    ))
    .await
    .expect("openrouter native-options request should succeed");

    let final_text = response_text(&response.output.content);
    let raw_output_items = response
        .raw_provider_response
        .as_ref()
        .and_then(|raw| raw["output"].as_array())
        .map_or(0, Vec::len);
    assert!(
        !final_text.trim().is_empty() || raw_output_items > 0,
        "expected either finalized response text or raw provider output items"
    );
    assert_usage_recorded(&response);

    if let Some(raw) = response.raw_provider_response.as_ref() {
        assert_json_number_close(&raw["temperature"], 0.65);
        assert_eq!(raw["max_output_tokens"], 96);
        assert_eq!(
            raw["metadata"]["trace_id"],
            "live-openrouter-native-options"
        );
    }
}
