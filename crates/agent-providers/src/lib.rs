//! Provider protocol adapters and translation primitives for `agent-toolkit`.
//!
//! This crate sits between the provider-agnostic request/response model in
//! `agent-core` and provider-specific wire protocols. It exposes:
//!
//! - [`adapter`] for built-in provider adapters and adapter selection.
//! - [`error`] for normalized adapter-layer errors.
//! - [`request_plan`] for transport/response execution contracts.
//! - [`stream_projector`] for projecting raw provider stream events into canonical
//!   stream events.
//! - [`openai_family`] and [`anthropic_family`] for provider-family payload types
//!   and spec-level error models.

pub mod adapter;
pub mod anthropic_family;
pub mod error;
mod family_codec;
pub mod openai_family;
mod refinement;
pub mod request_plan;
pub mod stream_projector;

#[cfg(test)]
mod fixture_tests;
