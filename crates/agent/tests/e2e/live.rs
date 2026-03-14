use std::env;
use std::future::Future;
use std::time::Duration;

use agent_toolkit::{ContentPart, ProviderId, ResponseMeta};

use super::timeout::with_timeout;

const LIVE_TEST_TIMEOUT: Duration = Duration::from_secs(45);

pub fn provider_api_key(provider: ProviderId) -> Option<String> {
    env::var(provider_api_key_env(provider))
        .ok()
        .filter(|value| !value.trim().is_empty())
}

pub fn provider_api_key_env(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::OpenAi => "OPENAI_API_KEY",
        ProviderId::Anthropic => "ANTHROPIC_API_KEY",
        ProviderId::OpenRouter => "OPENROUTER_API_KEY",
        ProviderId::GenericOpenAiCompatible => "OPENAI_API_KEY",
    }
}

pub fn require_provider_api_key(provider: ProviderId, test_name: &str) -> Option<String> {
    let api_key = provider_api_key(provider);
    if api_key.is_none() {
        eprintln!(
            "skipping {test_name}: {} is not set",
            provider_api_key_env(provider)
        );
    }
    api_key
}

pub fn default_live_model(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::OpenAi => "gpt-5-mini",
        ProviderId::Anthropic => "claude-sonnet-4-6",
        ProviderId::OpenRouter => "openai/gpt-5-nano",
        ProviderId::GenericOpenAiCompatible => "gpt-5-mini",
    }
}

pub async fn with_live_test_timeout<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    with_timeout(LIVE_TEST_TIMEOUT, future).await
}

pub fn response_text(parts: &[ContentPart]) -> String {
    parts.iter().fold(String::new(), |mut output, part| {
        if let ContentPart::Text { text } = part {
            output.push_str(text);
        }
        output
    })
}

pub fn assert_live_response_meta(meta: &ResponseMeta, provider: ProviderId) {
    assert_eq!(meta.selected_provider_kind, provider);
    assert!(
        !meta.selected_model.trim().is_empty(),
        "expected selected model metadata to be populated"
    );
    assert!(
        !meta.attempts.is_empty(),
        "expected at least one recorded attempt in response metadata"
    );
}
