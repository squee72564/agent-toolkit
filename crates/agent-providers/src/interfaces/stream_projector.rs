//! Streaming projection traits for provider-native event streams.

use agent_core::{CanonicalStreamEvent, ProviderRawStreamEvent};

use crate::error::AdapterError;

/// Projects raw provider streaming events into canonical stream events.
///
/// Implementations keep whatever incremental state they need between
/// [`Self::project`] calls and may emit trailing events from [`Self::finish`].
pub trait ProviderStreamProjector: Send {
    /// Consumes one raw provider stream event and emits zero or more canonical
    /// events.
    fn project(
        &mut self,
        raw: ProviderRawStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, AdapterError>;

    /// Finalizes the projector after the raw stream ends.
    ///
    /// Use this to flush buffered provider state into canonical terminal events.
    /// The default implementation emits no additional events.
    fn finish(&mut self) -> Result<Vec<CanonicalStreamEvent>, AdapterError> {
        Ok(Vec::new())
    }
}
