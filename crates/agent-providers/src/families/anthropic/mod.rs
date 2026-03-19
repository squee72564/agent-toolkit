//! Anthropic family implementation.
//!
//! This family owns the Anthropic codec, default stream projector, and the
//! underlying Messages API wire contract used across Anthropic integrations.

pub(crate) mod codec;
mod stream_projector;
pub(crate) mod wire;

#[cfg(test)]
mod tests;
