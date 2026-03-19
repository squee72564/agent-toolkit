//! Provider-specific overlays layered on top of family defaults.
//!
//! Each provider module contains request mutations, error refinements, and any
//! stream overrides that are specific to one concrete provider.

pub(crate) mod anthropic;
pub(crate) mod openai;
pub(crate) mod openai_compatible;
pub(crate) mod openrouter;
