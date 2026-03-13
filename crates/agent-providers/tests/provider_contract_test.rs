use agent_core::{CanonicalStreamEvent, ProviderId, ProviderRawStreamEvent};
use agent_providers::request_plan::{ProviderRequestPlan, TransportResponseFraming};
use agent_providers::streaming::ProviderStreamProjector;
use agent_transport::HttpRequestOptions;
use reqwest::{Method, header::HeaderMap};
use serde_json::json;

#[derive(Default)]
struct EchoProjector;

impl ProviderStreamProjector for EchoProjector {
    fn project(
        &mut self,
        raw: ProviderRawStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, agent_providers::error::AdapterError> {
        Ok(vec![CanonicalStreamEvent::ResponseStarted {
            model: Some(format!("{:?}", raw.provider)),
            response_id: Some(raw.sequence.to_string()),
        }])
    }
}

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
fn provider_stream_projector_trait_is_object_safe() {
    let mut projector: Box<dyn ProviderStreamProjector> = Box::new(EchoProjector);
    let raw = ProviderRawStreamEvent::from_sse(
        ProviderId::OpenAi,
        7,
        Some("response.created".to_string()),
        Some("evt-1".to_string()),
        Some(250),
        r#"{"type":"response.created"}"#,
    );

    let events = projector.project(raw).expect("projection should succeed");

    assert_eq!(
        events,
        vec![CanonicalStreamEvent::ResponseStarted {
            model: Some("OpenAi".to_string()),
            response_id: Some("7".to_string()),
        }]
    );
}
