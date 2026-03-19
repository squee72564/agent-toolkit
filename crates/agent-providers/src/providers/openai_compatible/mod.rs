//! Generic OpenAI-compatible provider refinements.
//!
//! This module holds the default overlay used for arbitrary or self-hosted
//! OpenAI-compatible endpoints that do not need branded provider quirks.

pub(crate) mod refinement;

#[cfg(test)]
mod tests;
