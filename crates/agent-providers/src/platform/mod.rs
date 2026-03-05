//! Provider-specific translation adapters and test harness wiring.

pub mod anthropic;
pub mod openai;
pub mod openrouter;

#[cfg(test)]
pub(crate) mod test_fixtures;
#[cfg(test)]
mod test_fixtures_test;
