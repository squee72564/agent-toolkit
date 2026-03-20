#[cfg(feature = "anthropic")]
mod anthropic;
pub(crate) mod base_client_builder;
mod common;
#[cfg(feature = "openai")]
mod openai;
#[cfg(feature = "openrouter")]
mod openrouter;

#[cfg(test)]
mod tests;

#[cfg(feature = "anthropic")]
pub use anthropic::{AnthropicClient, AnthropicClientBuilder, anthropic};
pub(crate) use base_client_builder::*;
#[cfg(feature = "openai")]
pub use openai::{OpenAiClient, OpenAiClientBuilder, openai};
#[cfg(feature = "openrouter")]
pub use openrouter::{OpenRouterClient, OpenRouterClientBuilder, openrouter};
