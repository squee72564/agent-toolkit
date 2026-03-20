//! Provider protocol adapters and translation primitives for `agent-toolkit`.
//!
//! This crate sits between the provider-agnostic request/response model in
//! `agent-core` and provider-specific wire protocols. External consumers should
//! treat this crate as a facade and prefer root-level imports such as
//! [`adapter_for`], [`ProviderAdapterHandle`], [`ProviderStreamProjectorHandle`],
//! [`AdapterError`], and [`ProviderRequestPlan`] rather than depending on the
//! internal module layout.
//!
//! It exposes:
//!
//! - [`adapter_for`] to obtain the built-in adapter for a concrete provider.
//! - [`ProviderAdapterHandle`] and [`ProviderStreamProjectorHandle`] as the
//!   closed runtime-facing provider contract.
//! - [`AdapterError`], [`AdapterErrorKind`], [`AdapterOperation`], and
//!   [`ProviderErrorInfo`] for normalized adapter-layer errors.
//! - [`ProviderRequestPlan`] and [`TransportResponseFraming`] for the transport
//!   request contract produced by adapters.
//!
//! Internally, built-in providers are composed in three layers:
//!
//! - adapter: runtime-facing composition root for one concrete provider
//! - family codec: protocol-family translation between canonical requests and
//!   family-native payloads
//! - refinement: provider-specific mutations and overrides layered on top of a
//!   family codec
//!
//! Family-shared protocol implementations live under `families`, while
//! provider-specific overlays live under `providers`.
//!
//! See `docs/provider-layering.md` for the full request and response flow.

mod adapter;
mod handles;
mod interfaces;
mod request_plan;

mod error;
mod families;
mod providers;

pub use adapter::adapter_for;
pub use error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo};
pub use handles::{ProviderAdapterHandle, ProviderStreamProjectorHandle};
pub use request_plan::{ProviderRequestPlan, TransportResponseFraming};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod fixture_tests;
