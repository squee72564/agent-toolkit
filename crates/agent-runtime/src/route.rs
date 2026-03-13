use crate::fallback::FallbackPolicy;
use crate::target::Target;

/// Ordered routed attempt chain for one logical call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// Primary target attempted first.
    pub primary: Target,
    /// Ordered fallback targets attempted after the primary target.
    pub fallbacks: Vec<Target>,
    /// Fallback decision policy evaluated between attempts.
    pub fallback_policy: FallbackPolicy,
}

impl Route {
    /// Creates a route with one primary target.
    pub fn to(primary: Target) -> Self {
        Self {
            primary,
            fallbacks: Vec::new(),
            fallback_policy: FallbackPolicy::default(),
        }
    }

    /// Appends one fallback target.
    pub fn with_fallback(mut self, target: Target) -> Self {
        self.fallbacks.push(target);
        self
    }

    /// Replaces the fallback target list.
    pub fn with_fallbacks(mut self, fallbacks: Vec<Target>) -> Self {
        self.fallbacks = fallbacks;
        self
    }

    /// Replaces the fallback decision policy.
    pub fn with_fallback_policy(mut self, fallback_policy: FallbackPolicy) -> Self {
        self.fallback_policy = fallback_policy;
        self
    }

    pub(crate) fn targets(&self) -> Vec<Target> {
        let mut targets = Vec::with_capacity(1 + self.fallbacks.len());
        targets.push(self.primary.clone());
        targets.extend(self.fallbacks.clone());
        targets
    }
}
