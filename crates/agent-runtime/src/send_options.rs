use std::{collections::BTreeMap, sync::Arc};

use crate::fallback::FallbackPolicy;
use crate::observer::RuntimeObserver;
use crate::target::Target;

/// Per-request routing and observability overrides.
///
/// This type intentionally keeps its fields public so callers can use struct
/// literals when needed. Equality compares observers by `Arc` identity rather
/// than observer contents.
#[derive(Clone, Default)]
pub struct SendOptions {
    /// Direct target override for the request.
    pub target: Option<Target>,
    /// Fallback policy override applied when routing across targets.
    pub fallback_policy: Option<FallbackPolicy>,
    /// Opaque request metadata forwarded to provider execution.
    pub metadata: BTreeMap<String, String>,
    /// Request-scoped observer override.
    ///
    /// `Debug` output redacts the observer internals, and `PartialEq` uses
    /// pointer identity for comparison.
    pub observer: Option<Arc<dyn RuntimeObserver>>,
}

impl SendOptions {
    /// Creates options that pin a request to a specific target.
    pub fn for_target(target: Target) -> Self {
        Self {
            target: Some(target),
            ..Self::default()
        }
    }

    /// Returns updated options with an explicit fallback policy.
    pub fn with_fallback_policy(mut self, fallback_policy: FallbackPolicy) -> Self {
        self.fallback_policy = Some(fallback_policy);
        self
    }

    /// Returns updated options with a request-scoped observer override.
    pub fn with_observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.observer = Some(observer);
        self
    }
}

impl std::fmt::Debug for SendOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendOptions")
            .field("target", &self.target)
            .field("fallback_policy", &self.fallback_policy)
            .field("metadata", &self.metadata)
            .field("observer", &self.observer.as_ref().map(|_| "configured"))
            .finish()
    }
}

impl PartialEq for SendOptions {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
            && self.fallback_policy == other.fallback_policy
            && self.metadata == other.metadata
            && match (&self.observer, &other.observer) {
                (Some(lhs), Some(rhs)) => Arc::ptr_eq(lhs, rhs),
                (None, None) => true,
                _ => false,
            }
    }
}

impl Eq for SendOptions {}
