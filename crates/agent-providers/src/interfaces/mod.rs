//! Core architecture boundaries shared across provider implementations.
//!
//! This module defines the traits that connect adapters, family codecs,
//! provider refinements, and stream projectors.

mod adapter;
mod family_codec;
mod refinement;
mod stream_projector;

pub use adapter::ProviderAdapter;
pub(crate) use family_codec::*;
pub(crate) use refinement::*;
pub use stream_projector::ProviderStreamProjector;
