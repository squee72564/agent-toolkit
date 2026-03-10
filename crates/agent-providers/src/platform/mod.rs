//! Provider-specific translation adapters and test harness wiring.

/// Anthropic provider-family translation implementation.
pub mod anthropic;
/// OpenAI provider-family translation implementation.
pub mod openai;
/// OpenRouter translation implementation layered on the OpenAI-style protocol.
pub mod openrouter;

#[cfg(test)]
pub(crate) mod test_fixtures;
#[cfg(test)]
mod test_fixtures_test;
