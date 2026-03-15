use crate::attempt::AttemptSpec;
use crate::fallback::FallbackPolicy;
use crate::planner::PlanningRejectionPolicy;

/// Ordered routed attempt chain for one logical call.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Primary target attempted first.
    pub primary: AttemptSpec,
    /// Ordered fallback targets attempted after the primary target.
    pub fallbacks: Vec<AttemptSpec>,
    /// Fallback decision policy evaluated between attempts.
    pub fallback_policy: FallbackPolicy,
    /// Planning-time behavior for skipped/rejected attempts.
    pub planning_rejection_policy: PlanningRejectionPolicy,
}

impl Route {
    /// Creates a route with one primary target.
    pub fn to(primary: impl Into<AttemptSpec>) -> Self {
        Self {
            primary: primary.into(),
            fallbacks: Vec::new(),
            fallback_policy: FallbackPolicy::default(),
            planning_rejection_policy: PlanningRejectionPolicy::FailFast,
        }
    }

    /// Appends one fallback target.
    pub fn with_fallback(mut self, target: impl Into<AttemptSpec>) -> Self {
        self.fallbacks.push(target.into());
        self
    }

    /// Replaces the fallback target list.
    pub fn with_fallbacks(mut self, fallbacks: Vec<AttemptSpec>) -> Self {
        self.fallbacks = fallbacks;
        self
    }

    /// Replaces the fallback decision policy.
    pub fn with_fallback_policy(mut self, fallback_policy: FallbackPolicy) -> Self {
        self.fallback_policy = fallback_policy;
        self
    }

    /// Replaces the planning-rejection policy.
    pub fn with_planning_rejection_policy(
        mut self,
        planning_rejection_policy: PlanningRejectionPolicy,
    ) -> Self {
        self.planning_rejection_policy = planning_rejection_policy;
        self
    }

    pub(crate) fn attempts(&self) -> Vec<AttemptSpec> {
        let mut attempts = Vec::with_capacity(1 + self.fallbacks.len());
        attempts.push(self.primary.clone());
        attempts.extend(self.fallbacks.clone());
        attempts
    }
}
