//! Shared protocol-family implementations reused across providers.
//!
//! Each family module owns the default codec, default stream projector, and the
//! lower-level wire contract for providers that speak that family.

pub(crate) mod anthropic;
pub(crate) mod openai_compatible;
