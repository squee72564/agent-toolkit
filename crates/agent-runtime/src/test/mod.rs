use std::collections::HashMap;

use agent_core::{
    ProviderId, ResponseFormat, ToolChoice,
    types::{ContentPart, Message, MessageRole, ToolResultContent},
};
use agent_providers::adapter::adapter_for;
use agent_transport::HttpTransport;
use serde_json::json;

use crate::provider_client::ProviderClient;
use crate::provider_runtime::ProviderRuntime;

use super::*;

mod agent_toolkit_test;
mod clients_test;
mod conversation_test;
mod fallback_test;
mod message_create_input_test;
mod observer_test;
mod provider_client_test;
mod provider_config_test;
mod provider_runtime_test;
mod provider_stream_runtime_test;
mod runtime_error_test;
mod send_options_test;
mod streaming_api_test;
mod types_test;

fn runtime_error(
    kind: RuntimeErrorKind,
    provider: Option<ProviderId>,
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
        source: None,
    }
}

fn test_provider_client(provider: ProviderId) -> ProviderClient {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let platform = adapter
        .platform_config("http://127.0.0.1:1".to_string())
        .expect("test platform should build");

    ProviderClient::new(ProviderRuntime {
        provider,
        adapter,
        platform,
        auth_token: "test-key".to_string(),
        default_model: None,
        transport,
        observer: None,
    })
}

#[derive(Debug)]
struct ObserverStub;

impl RuntimeObserver for ObserverStub {}

fn terminal_failure_error(error: &RuntimeError) -> &RuntimeError {
    crate::types::terminal_failure_error(error)
}
