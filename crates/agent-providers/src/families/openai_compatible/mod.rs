//! OpenAI-compatible family implementation.
//!
//! This family owns the shared codec, default stream projector, and the common
//! wire contract reused by OpenAI-compatible providers.

pub(crate) mod codec;
mod stream_projector;
pub(crate) mod wire;

#[cfg(test)]
mod tests;
