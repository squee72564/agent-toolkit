//! Canonical and raw streaming event types shared by provider adapters and the runtime.

/// Canonical streaming events derived from provider-specific raw events.
pub mod canonical;
/// Output item descriptors carried by canonical stream events.
pub mod item;
/// Provider-native raw stream frames and helpers.
pub mod raw;

/// Re-export of [`canonical::CanonicalStreamEnvelope`].
pub use canonical::CanonicalStreamEnvelope;
/// Re-export of [`canonical::CanonicalStreamEvent`].
pub use canonical::CanonicalStreamEvent;
/// Re-export of [`item::StreamOutputItemEnd`].
pub use item::StreamOutputItemEnd;
/// Re-export of [`item::StreamOutputItemStart`].
pub use item::StreamOutputItemStart;
/// Re-export of [`raw::ProviderRawStreamEvent`].
pub use raw::ProviderRawStreamEvent;
/// Re-export of [`raw::RawStreamPayload`].
pub use raw::RawStreamPayload;
/// Re-export of [`raw::RawStreamTransport`].
pub use raw::RawStreamTransport;
