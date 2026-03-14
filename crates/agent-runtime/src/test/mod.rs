use std::collections::HashMap;

use agent_core::{
    ExecutionPlan, PlatformConfig, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId,
    ProviderId, ProviderInstanceId, ProviderKind, Response, ResponseFormat, ToolChoice,
    types::{ContentPart, Message, MessageRole, ToolResultContent},
};
use agent_providers::adapter::{ProviderAdapter, adapter_for};
use agent_providers::error::{AdapterError, ProviderErrorInfo};
use agent_providers::streaming::ProviderStreamProjector;
use agent_transport::HttpTransport;
use reqwest::header::{HeaderMap, HeaderName};
use serde_json::Value;
use serde_json::json;

use crate::provider_client::ProviderClient;
use crate::provider_config::ProviderConfig;
use crate::provider_runtime::ProviderRuntime;
use crate::registered_provider::RegisteredProvider;
use crate::target::Target;

use super::*;

mod agent_toolkit_test;
mod client_native_options_test;
mod clients_test;
mod conversation_test;
mod fallback_test;
mod message_create_input_test;
mod observer_test;
mod planner_test;
mod provider_client_test;
mod provider_config_test;
mod provider_runtime_test;
mod provider_stream_runtime_test;
mod registered_provider_test;
mod route_attempts_test;
mod runtime_error_test;
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
        executed_failure_meta: None,
        source: None,
    }
}

pub(crate) fn default_instance_id(provider: ProviderKind) -> ProviderInstanceId {
    match provider {
        ProviderKind::OpenAi => ProviderInstanceId::openai_default(),
        ProviderKind::Anthropic => ProviderInstanceId::anthropic_default(),
        ProviderKind::OpenRouter => ProviderInstanceId::openrouter_default(),
        ProviderKind::GenericOpenAiCompatible => {
            ProviderInstanceId::generic_openai_compatible_default()
        }
    }
}

fn test_provider_client(provider: ProviderId) -> ProviderClient {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let instance_id = default_instance_id(provider);
    let registered = RegisteredProvider::new(
        instance_id.clone(),
        provider,
        ProviderConfig::new("test-key"),
    );
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

    ProviderClient::new(ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    })
}

fn test_provider_client_with_base_url(
    provider: ProviderId,
    base_url: &str,
    default_model: Option<&str>,
) -> ProviderClient {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let instance_id = default_instance_id(provider);
    let mut config = ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = RegisteredProvider::new(instance_id.clone(), provider, config);
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

    ProviderClient::new(ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    })
}

fn test_provider_client_with_streaming_support(
    provider: ProviderId,
    default_model: Option<&str>,
    supports_streaming: bool,
) -> ProviderClient {
    static STREAMING_DISABLED_ADAPTER: StreamingCapabilityTestAdapter =
        StreamingCapabilityTestAdapter {
            supports_streaming: false,
        };
    static STREAMING_ENABLED_ADAPTER: StreamingCapabilityTestAdapter =
        StreamingCapabilityTestAdapter {
            supports_streaming: true,
        };

    let adapter: &'static dyn ProviderAdapter = if supports_streaming {
        &STREAMING_ENABLED_ADAPTER
    } else {
        &STREAMING_DISABLED_ADAPTER
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let platform = PlatformConfig {
        protocol: adapter.descriptor().protocol.clone(),
        base_url: "http://127.0.0.1:1".to_string(),
        auth_style: adapter.descriptor().default_auth_style.clone(),
        request_id_header: adapter.descriptor().default_request_id_header.clone(),
        default_headers: adapter.descriptor().default_headers.clone(),
    };
    let instance_id = default_instance_id(provider);
    let mut config = ProviderConfig::new("test-key");
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = RegisteredProvider::new(instance_id.clone(), provider, config);

    ProviderClient::new(ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
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

fn route_planning_failure(error: &RuntimeError) -> &RoutePlanningFailure {
    error
        .source_ref()
        .and_then(|source| source.downcast_ref::<RoutePlanningFailure>())
        .expect("runtime error should wrap RoutePlanningFailure")
}

fn executed_failure_meta(error: &RuntimeError) -> &ExecutedFailureMeta {
    error
        .executed_failure_meta()
        .expect("runtime error should carry ExecutedFailureMeta")
}

#[derive(Debug)]
struct StreamingCapabilityTestAdapter {
    supports_streaming: bool,
}

impl ProviderAdapter for StreamingCapabilityTestAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAi
    }

    fn descriptor(&self) -> &ProviderDescriptor {
        if self.supports_streaming {
            static DESCRIPTOR: std::sync::LazyLock<ProviderDescriptor> =
                std::sync::LazyLock::new(|| ProviderDescriptor {
                    kind: ProviderKind::OpenAi,
                    family: ProviderFamilyId::OpenAiCompatible,
                    protocol: agent_core::ProtocolKind::OpenAI,
                    default_base_url: "https://api.openai.com",
                    endpoint_path: "/v1/responses",
                    default_auth_style: agent_core::AuthStyle::Bearer,
                    default_request_id_header: HeaderName::from_static("x-request-id"),
                    default_headers: HeaderMap::new(),
                    capabilities: ProviderCapabilities {
                        supports_streaming: true,
                        supports_family_native_options: true,
                        supports_provider_native_options: true,
                    },
                });
            &DESCRIPTOR
        } else {
            static DESCRIPTOR: std::sync::LazyLock<ProviderDescriptor> =
                std::sync::LazyLock::new(|| ProviderDescriptor {
                    kind: ProviderKind::OpenAi,
                    family: ProviderFamilyId::OpenAiCompatible,
                    protocol: agent_core::ProtocolKind::OpenAI,
                    default_base_url: "https://api.openai.com",
                    endpoint_path: "/v1/responses",
                    default_auth_style: agent_core::AuthStyle::Bearer,
                    default_request_id_header: HeaderName::from_static("x-request-id"),
                    default_headers: HeaderMap::new(),
                    capabilities: ProviderCapabilities {
                        supports_streaming: false,
                        supports_family_native_options: true,
                        supports_provider_native_options: true,
                    },
                });
            &DESCRIPTOR
        }
    }

    fn plan_request(
        &self,
        execution: &ExecutionPlan,
    ) -> Result<agent_providers::request_plan::ProviderRequestPlan, AdapterError> {
        adapter_for(ProviderId::OpenAi).plan_request(execution)
    }

    fn decode_response_json(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        adapter_for(ProviderId::OpenAi).decode_response_json(body, requested_format)
    }

    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        adapter_for(ProviderId::OpenAi).decode_error(body)
    }

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        adapter_for(ProviderId::OpenAi).create_stream_projector()
    }
}
