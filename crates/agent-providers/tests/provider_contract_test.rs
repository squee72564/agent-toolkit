use reqwest::{Method, header::HeaderMap};
use serde_json::json;

use agent_core::{
    CanonicalStreamEvent, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId, ProviderKind,
    ProviderRawStreamEvent,
};
use agent_providers::{ProviderRequestPlan, TransportResponseFraming, adapter_for};
use agent_transport::HttpRequestOptions;

#[test]
fn provider_request_plan_carries_transport_and_response_contract() {
    let plan = ProviderRequestPlan {
        body: json!({ "stream": true }),
        warnings: Vec::new(),
        method: Method::POST,
        response_framing: TransportResponseFraming::Sse,
        endpoint_path_override: Some("/override".to_string()),
        provider_headers: HeaderMap::new(),
        request_options: HttpRequestOptions::sse_defaults(),
    };

    assert_eq!(plan.method, Method::POST);
    assert_eq!(plan.response_framing, TransportResponseFraming::Sse);
    assert_eq!(plan.endpoint_path_override.as_deref(), Some("/override"));
    assert_eq!(plan.body["stream"], json!(true));
}

#[test]
fn provider_stream_projector_handle_projects_builtin_stream_events() {
    let mut projector = adapter_for(ProviderKind::OpenAi).create_stream_projector();
    let raw = ProviderRawStreamEvent::from_sse(
        ProviderKind::OpenAi,
        1,
        Some("response.created".to_string()),
        Some("evt-1".to_string()),
        Some(250),
        r#"{"type":"response.created","response":{"id":"resp_1","model":"gpt-5-mini"}}"#,
    );

    let events = projector.project(raw).expect("projection should succeed");

    assert_eq!(
        events,
        vec![CanonicalStreamEvent::ResponseStarted {
            model: Some("gpt-5-mini".to_string()),
            response_id: Some("resp_1".to_string()),
        }]
    );
}

#[cfg(feature = "test-support")]
#[test]
fn test_support_adapter_can_override_descriptor_and_delegate_builtin_behavior() {
    let descriptor = ProviderDescriptor {
        kind: ProviderKind::OpenAi,
        family: ProviderFamilyId::OpenAiCompatible,
        protocol: agent_core::ProtocolKind::OpenAI,
        default_base_url: "https://api.openai.com",
        endpoint_path: "/v1/responses",
        default_auth_style: agent_core::AuthStyle::Bearer,
        default_request_id_header: reqwest::header::HeaderName::from_static("x-request-id"),
        default_headers: HeaderMap::new(),
        capabilities: ProviderCapabilities {
            supports_streaming: false,
            supports_family_native_options: true,
            supports_provider_native_options: true,
        },
    };

    let adapter =
        agent_providers::test_support::TestAdapterBuilder::new(descriptor, |_execution| {
            Ok(ProviderRequestPlan {
                body: json!({ "custom": true }),
                warnings: Vec::new(),
                method: Method::POST,
                response_framing: TransportResponseFraming::Json,
                endpoint_path_override: Some("/custom".to_string()),
                provider_headers: HeaderMap::new(),
                request_options: HttpRequestOptions::default(),
            })
        })
        .delegate_to_builtin(ProviderKind::OpenAi)
        .build();

    assert!(!adapter.capabilities().supports_streaming);
    assert_eq!(adapter.descriptor().endpoint_path, "/v1/responses");

    let mut projector = adapter.create_stream_projector();
    let events = projector
        .project(ProviderRawStreamEvent::from_sse(
            ProviderKind::OpenAi,
            2,
            Some("response.created".to_string()),
            Some("evt-2".to_string()),
            Some(250),
            r#"{"type":"response.created","response":{"id":"resp_2","model":"gpt-5-mini"}}"#,
        ))
        .expect("delegated projector should succeed");

    assert_eq!(
        events,
        vec![CanonicalStreamEvent::ResponseStarted {
            model: Some("gpt-5-mini".to_string()),
            response_id: Some("resp_2".to_string()),
        }]
    );
}
