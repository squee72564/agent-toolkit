use std::sync::Arc;

use serde_json::Value;

use agent_core::{ExecutionPlan, ProviderDescriptor, ProviderKind, Response, ResponseFormat};

use crate::{
    AdapterError, ProviderAdapterHandle, ProviderErrorInfo, ProviderRequestPlan,
    ProviderStreamProjectorHandle, adapter::adapter_impl_for, interfaces::ProviderAdapter,
};

type PlanRequestFn =
    dyn Fn(&ExecutionPlan) -> Result<ProviderRequestPlan, AdapterError> + Send + Sync + 'static;
type DecodeResponseFn =
    dyn Fn(Value, &ResponseFormat) -> Result<Response, AdapterError> + Send + Sync + 'static;
type DecodeErrorFn = dyn Fn(&Value) -> Option<ProviderErrorInfo> + Send + Sync + 'static;
type CreateProjectorFn = dyn Fn() -> ProviderStreamProjectorHandle + Send + Sync + 'static;

/// Builder for workspace test adapters backed by private provider interfaces.
pub struct TestAdapterBuilder {
    descriptor: ProviderDescriptor,
    plan_request: Arc<PlanRequestFn>,
    decode_response_json: Option<Arc<DecodeResponseFn>>,
    decode_error: Option<Arc<DecodeErrorFn>>,
    create_stream_projector: Option<Arc<CreateProjectorFn>>,
}

impl TestAdapterBuilder {
    /// Creates a test adapter builder with a fixed descriptor and plan function.
    pub fn new(
        descriptor: ProviderDescriptor,
        plan_request: impl Fn(&ExecutionPlan) -> Result<ProviderRequestPlan, AdapterError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            descriptor,
            plan_request: Arc::new(plan_request),
            decode_response_json: None,
            decode_error: None,
            create_stream_projector: None,
        }
    }

    /// Delegates decode and projector behavior to one built-in provider adapter.
    pub fn delegate_to_builtin(mut self, kind: ProviderKind) -> Self {
        let adapter = crate::adapter_for(kind);
        self.decode_response_json = Some(Arc::new(move |body, requested_format| {
            adapter.decode_response_json(body, requested_format)
        }));
        self.decode_error = Some(Arc::new(move |body| adapter.decode_error(body)));
        self.create_stream_projector = Some(Arc::new(move || adapter.create_stream_projector()));
        self
    }

    /// Overrides non-streaming response decoding.
    pub fn with_decode_response_json(
        mut self,
        decode: impl Fn(Value, &ResponseFormat) -> Result<Response, AdapterError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.decode_response_json = Some(Arc::new(decode));
        self
    }

    /// Overrides provider error decoding.
    pub fn with_decode_error(
        mut self,
        decode: impl Fn(&Value) -> Option<ProviderErrorInfo> + Send + Sync + 'static,
    ) -> Self {
        self.decode_error = Some(Arc::new(decode));
        self
    }

    /// Overrides stream projector creation.
    pub fn with_stream_projector(
        mut self,
        create: impl Fn() -> ProviderStreamProjectorHandle + Send + Sync + 'static,
    ) -> Self {
        self.create_stream_projector = Some(Arc::new(create));
        self
    }

    /// Builds a reusable adapter handle for workspace tests.
    pub fn build(self) -> ProviderAdapterHandle {
        ProviderAdapterHandle::from_raw(Box::leak(Box::new(TestAdapter {
            descriptor: self.descriptor,
            plan_request: self.plan_request,
            decode_response_json: self.decode_response_json,
            decode_error: self.decode_error,
            create_stream_projector: self.create_stream_projector,
        })))
    }
}

#[derive(Clone)]
struct TestAdapter {
    descriptor: ProviderDescriptor,
    plan_request: Arc<PlanRequestFn>,
    decode_response_json: Option<Arc<DecodeResponseFn>>,
    decode_error: Option<Arc<DecodeErrorFn>>,
    create_stream_projector: Option<Arc<CreateProjectorFn>>,
}

impl std::fmt::Debug for TestAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestAdapter")
            .field("kind", &self.descriptor.kind)
            .finish_non_exhaustive()
    }
}

impl ProviderAdapter for TestAdapter {
    fn kind(&self) -> ProviderKind {
        self.descriptor.kind
    }

    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn plan_request(&self, execution: &ExecutionPlan) -> Result<ProviderRequestPlan, AdapterError> {
        (self.plan_request)(execution)
    }

    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        if let Some(decode) = &self.decode_response_json {
            return decode(body, requested_format);
        }

        adapter_impl_for(self.kind()).decode_response_json(body, requested_format)
    }

    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        if let Some(decode) = &self.decode_error {
            return decode(body);
        }

        adapter_impl_for(self.kind()).decode_error(body)
    }

    fn create_stream_projector(&self) -> Box<dyn crate::interfaces::ProviderStreamProjector> {
        if let Some(create) = &self.create_stream_projector {
            return create().into_raw();
        }

        adapter_impl_for(self.kind()).create_stream_projector()
    }
}
