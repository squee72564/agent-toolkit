//! Provider protocol adapters and translation primitives for `agent-toolkit`.
//!
//! This crate sits between the provider-agnostic request/response model in
//! `agent-core` and provider-specific wire protocols. It exposes:
//!
//! - [`adapter`] for built-in provider adapters and adapter selection.
//! - [`error`] for normalized adapter-layer errors.
//! - [`request_plan`] for transport/response execution contracts.
//! - [`streaming`] for projecting raw provider stream events into canonical
//!   stream events.
//! - [`openai_family`] and [`anthropic_family`] for provider-family payload types
//!   and spec-level error models.

pub mod adapter;
pub mod anthropic_family;
pub mod error;
pub mod openai_family;
pub mod platform;
pub mod request_plan;
pub mod streaming;
