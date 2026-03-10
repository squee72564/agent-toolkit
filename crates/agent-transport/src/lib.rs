//! HTTP transport primitives shared across the agent-toolkit workspace.
//!
//! This crate currently exposes an HTTP transport with support for:
//! - request construction from [`agent_core::PlatformConfig`] and [`agent_core::AdapterContext`]
//! - retrying retryable HTTP responses before a response body is consumed
//! - JSON, raw-bytes, and server-sent events (SSE) response modes
//! - configurable request, stream setup, and stream idle timeouts
//!
//! Most consumers only need [`HttpTransport`] and its convenience methods such as
//! [`HttpTransport::get_json`], [`HttpTransport::post_json`], or [`HttpTransport::post_sse`].

/// HTTP transport types and helpers.
pub mod http;

#[doc(inline)]
pub use crate::http::*;
