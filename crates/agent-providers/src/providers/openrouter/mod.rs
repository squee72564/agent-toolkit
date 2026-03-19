//! OpenRouter-specific refinements and stream overrides layered on top of the
//! OpenAI-compatible family.

pub(crate) mod refinement;
pub(crate) mod stream_projector;

#[cfg(test)]
mod tests;
