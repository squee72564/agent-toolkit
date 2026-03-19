//! Adapter Interface
use serde_json::Value;

use crate::error::{AdapterError, ProviderErrorInfo};
use crate::interfaces::ProviderStreamProjector;
use crate::request_plan::ProviderRequestPlan;
use agent_core::{
    ExecutionPlan, ProviderCapabilities, ProviderDescriptor, ProviderKind, Response, ResponseFormat,
};

/// Runtime-facing boundary for one concrete provider integration.
///
/// Adapters compose a provider family codec with provider-specific refinements,
/// then expose the final request-planning, response-decoding, and streaming
/// entrypoints consumed by `agent-runtime`.
pub trait ProviderAdapter: Sync + std::fmt::Debug {
    /// Returns the concrete provider this adapter serves.
    fn kind(&self) -> ProviderKind;
    /// Returns the provider descriptor used by routing and transport setup.
    fn descriptor(&self) -> &ProviderDescriptor;
    /// Returns the advertised capabilities for this provider.
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.descriptor().capabilities
    }
    /// Converts an [`ExecutionPlan`] into the final provider request contract.
    fn plan_request(&self, execution: &ExecutionPlan) -> Result<ProviderRequestPlan, AdapterError>;
    /// Decodes a non-streaming provider response body into canonical output.
    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError>;
    /// Extracts provider-specific error metadata from a raw error body.
    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo>;
    /// Creates the stream projector used for this provider's streaming protocol.
    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector>;
}
