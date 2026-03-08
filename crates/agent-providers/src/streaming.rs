use agent_core::{CanonicalStreamEvent, ProviderRawStreamEvent};

use crate::error::AdapterError;

pub trait ProviderStreamProjector: Send {
    fn project(
        &mut self,
        raw: ProviderRawStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, AdapterError>;

    fn finish(&mut self) -> Result<Vec<CanonicalStreamEvent>, AdapterError> {
        Ok(Vec::new())
    }
}
