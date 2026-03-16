use crate::routing::RoutePlanningFailure;
use crate::{ExecutedFailureMeta, RuntimeError};

pub(crate) fn terminal_failure_error(error: &RuntimeError) -> &RuntimeError {
    crate::types::terminal_failure_error(error)
}

pub(crate) fn route_planning_failure(error: &RuntimeError) -> &RoutePlanningFailure {
    error
        .source_ref()
        .and_then(|source| source.downcast_ref::<RoutePlanningFailure>())
        .expect("runtime error should wrap RoutePlanningFailure")
}

pub(crate) fn executed_failure_meta(error: &RuntimeError) -> &ExecutedFailureMeta {
    error
        .executed_failure_meta()
        .expect("runtime error should carry ExecutedFailureMeta")
}
