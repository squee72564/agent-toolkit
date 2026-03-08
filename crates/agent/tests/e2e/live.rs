use std::env;

use agent_toolkit::{ProviderId, RuntimeObserver};

pub fn provider_api_key(provider: ProviderId) -> Option<String> {
    let env_name = match provider {
        ProviderId::OpenAi => "OPENAI_API_KEY",
        ProviderId::Anthropic => "ANTHROPIC_API_KEY",
        ProviderId::OpenRouter => "OPENROUTER_API_KEY",
    };

    env::var(env_name).ok().filter(|value| !value.trim().is_empty())
}

pub fn maybe_observer_event_count(observer: &impl RuntimeObserver) -> usize {
    let _ = observer;
    0
}
