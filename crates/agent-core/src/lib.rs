//! Core request, response, and streaming types shared across the agent workspace.
//!
//! `agent-core` defines the stable data model passed between higher-level runtime APIs,
//! provider adapters, and transport implementations. It intentionally avoids provider-specific
//! wire formats in favor of:
//!
//! - request and response types used by runtime and adapter boundaries
//! - message and tool-call primitives shared across providers
//! - canonical streaming events that normalize raw provider stream payloads

/// Streaming event types shared between provider adapters and the runtime.
pub mod stream;
/// Provider-agnostic request, response, message, and platform types.
pub mod types;

/// Re-export of the canonical streaming surface.
pub use stream::*;
/// Re-export of the core request and response surface.
pub use types::*;
