use std::{collections::BTreeMap, sync::Arc};

use crate::fallback::FallbackPolicy;
use crate::observer::RuntimeObserver;
use crate::target::Target;

#[derive(Clone, Default)]
pub struct SendOptions {
    pub target: Option<Target>,
    pub fallback_policy: Option<FallbackPolicy>,
    pub metadata: BTreeMap<String, String>,
    pub observer: Option<Arc<dyn RuntimeObserver>>,
}

impl SendOptions {
    pub fn for_target(target: Target) -> Self {
        Self {
            target: Some(target),
            ..Self::default()
        }
    }

    pub fn with_fallback_policy(mut self, fallback_policy: FallbackPolicy) -> Self {
        self.fallback_policy = Some(fallback_policy);
        self
    }

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
