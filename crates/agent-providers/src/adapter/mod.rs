//! Built-in provider adapters and adapter selection.
//!
//! This module is the public entrypoint for runtime-facing provider integration.
//! Call [`adapter_for`] to obtain the adapter for a concrete
//! [`agent_core::ProviderKind`], then use
//! [`crate::interfaces::ProviderAdapter`] to plan requests, decode responses,
//! and create stream projectors.

#[cfg(test)]
mod tests;

pub(crate) mod anthropic;
pub(crate) mod core;
pub(crate) mod openai;
pub(crate) mod openrouter;

pub use anthropic::*;
pub use core::*;
pub use openai::*;
pub use openrouter::*;
