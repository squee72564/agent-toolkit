use std::time::Duration;

use agent_core::ProviderId;

use crate::RuntimeErrorKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestStartEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
    pub first_target: Option<ProviderId>,
    pub resolved_target_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptStartEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptSuccessEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptFailureEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
    pub error_kind: Option<RuntimeErrorKind>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestEndEvent {
    pub request_id: Option<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub target_index: Option<usize>,
    pub attempt_index: Option<usize>,
    pub elapsed: Duration,
    pub status_code: Option<u16>,
    pub error_kind: Option<RuntimeErrorKind>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptMeta {
    pub provider: ProviderId,
    pub model: String,
    pub success: bool,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub error_kind: Option<RuntimeErrorKind>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseMeta {
    pub selected_provider: ProviderId,
    pub selected_model: String,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub attempts: Vec<AttemptMeta>,
}
