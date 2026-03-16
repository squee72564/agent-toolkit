use std::{collections::BTreeMap, sync::Arc};

pub use agent_core::ResponseMode;

use crate::observability::RuntimeObserver;

/// Typed route-wide transport controls.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransportOptions {
    /// Override used only when extracting request ids from responses.
    pub request_id_header_override: Option<String>,
    /// Extra outbound headers applied route-wide.
    pub extra_headers: BTreeMap<String, String>,
}

/// Route-wide execution behavior independent of routing policy.
#[derive(Clone, Default)]
pub struct ExecutionOptions {
    /// Response delivery mode for the whole call.
    pub response_mode: ResponseMode,
    /// Request-scoped observer override.
    pub observer: Option<Arc<dyn RuntimeObserver>>,
    /// Route-wide typed transport controls.
    pub transport: TransportOptions,
}

impl std::fmt::Debug for ExecutionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionOptions")
            .field("response_mode", &self.response_mode)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .field("transport", &self.transport)
            .finish()
    }
}

impl PartialEq for ExecutionOptions {
    fn eq(&self, other: &Self) -> bool {
        self.response_mode == other.response_mode
            && self.transport == other.transport
            && match (&self.observer, &other.observer) {
                (Some(lhs), Some(rhs)) => Arc::ptr_eq(lhs, rhs),
                (None, None) => true,
                _ => false,
            }
    }
}

impl Eq for ExecutionOptions {}
