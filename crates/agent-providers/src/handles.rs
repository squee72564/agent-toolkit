use serde_json::Value;

use agent_core::{
    CanonicalStreamEvent, ExecutionPlan, ProviderCapabilities, ProviderDescriptor, ProviderKind,
    ProviderRawStreamEvent, Response, ResponseFormat,
};

use crate::{
    AdapterError, ProviderErrorInfo, ProviderRequestPlan,
    interfaces::{ProviderAdapter, ProviderStreamProjector},
};

/// Closed runtime-facing handle for a built-in provider adapter.
#[derive(Clone, Copy)]
pub struct ProviderAdapterHandle {
    inner: &'static dyn ProviderAdapter,
}

impl ProviderAdapterHandle {
    pub(crate) fn from_raw(inner: &'static dyn ProviderAdapter) -> Self {
        Self { inner }
    }

    /// Returns the concrete provider this handle serves.
    pub fn kind(self) -> ProviderKind {
        self.inner.kind()
    }

    /// Returns the provider descriptor used by routing and transport setup.
    pub fn descriptor(self) -> &'static ProviderDescriptor {
        self.inner.descriptor()
    }

    /// Returns the advertised capabilities for this provider.
    pub fn capabilities(self) -> &'static ProviderCapabilities {
        self.inner.capabilities()
    }

    /// Converts an [`ExecutionPlan`] into the final provider request contract.
    pub fn plan_request(
        self,
        execution: &ExecutionPlan,
    ) -> Result<ProviderRequestPlan, AdapterError> {
        self.inner.plan_request(execution)
    }

    /// Decodes a non-streaming provider response body into canonical output.
    pub fn decode_response_json(
        self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        self.inner.decode_response_json(body, requested_format)
    }

    /// Extracts provider-specific error metadata from a raw error body.
    pub fn decode_error(self, body: &Value) -> Option<ProviderErrorInfo> {
        self.inner.decode_error(body)
    }

    /// Creates the stream projector used for this provider's streaming protocol.
    pub fn create_stream_projector(self) -> ProviderStreamProjectorHandle {
        ProviderStreamProjectorHandle::from_raw(self.inner.create_stream_projector())
    }
}

impl std::fmt::Debug for ProviderAdapterHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderAdapterHandle")
            .field("kind", &self.kind())
            .finish()
    }
}

/// Closed runtime-facing handle for a provider stream projector.
pub struct ProviderStreamProjectorHandle {
    inner: Box<dyn ProviderStreamProjector>,
}

impl ProviderStreamProjectorHandle {
    pub(crate) fn from_raw(inner: Box<dyn ProviderStreamProjector>) -> Self {
        Self { inner }
    }

    pub(crate) fn into_raw(self) -> Box<dyn ProviderStreamProjector> {
        self.inner
    }

    /// Consumes one raw provider stream event and emits zero or more canonical events.
    pub fn project(
        &mut self,
        raw: ProviderRawStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, AdapterError> {
        self.inner.project(raw)
    }

    /// Finalizes the projector after the raw stream ends.
    pub fn finish(&mut self) -> Result<Vec<CanonicalStreamEvent>, AdapterError> {
        self.inner.finish()
    }
}

impl std::fmt::Debug for ProviderStreamProjectorHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderStreamProjectorHandle")
            .finish_non_exhaustive()
    }
}
