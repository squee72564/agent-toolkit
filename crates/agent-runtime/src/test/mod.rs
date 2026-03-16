use agent_core::ProviderKind;

use crate::{RuntimeError, RuntimeErrorKind};

pub(crate) mod error_fixtures;
pub(crate) mod observer_fixtures;
pub(crate) mod provider_test_fixtures;
mod runtime_error_test;
mod types_test;

pub(crate) use error_fixtures::{
    executed_failure_meta, route_planning_failure, terminal_failure_error,
};
pub(crate) use provider_test_fixtures::{
    default_instance_id, spawn_sse_stub, test_provider_client, test_provider_client_with_base_url,
    test_provider_client_with_streaming_support, test_provider_runtime,
};

pub(crate) fn runtime_error(
    kind: RuntimeErrorKind,
    provider: Option<ProviderKind>,
    status_code: Option<u16>,
    provider_code: Option<&str>,
) -> RuntimeError {
    RuntimeError {
        kind,
        message: "test error".to_string(),
        provider,
        status_code,
        request_id: None,
        provider_code: provider_code.map(ToString::to_string),
        executed_failure_meta: None,
        source: None,
    }
}
