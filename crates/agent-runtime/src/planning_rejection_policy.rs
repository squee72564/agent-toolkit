/// Routing behavior for attempt rejection during planning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlanningRejectionPolicy {
    /// Stop routing immediately when the current attempt is rejected during planning.
    #[default]
    FailFast,
    /// Skip the rejected target and continue with the next configured attempt.
    SkipRejectedTargets,
}
