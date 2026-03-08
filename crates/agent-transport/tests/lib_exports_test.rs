use std::time::Duration;

use agent_transport::{
    HttpSseResponse, HttpTransport, HttpTransportBuilder, RetryPolicy, SseEvent, TransportError,
};

fn assert_builder_type(_: HttpTransportBuilder) {}
fn assert_transport_type(_: HttpTransport) {}
fn assert_response_type(_: agent_transport::HttpJsonResponse) {}
fn assert_sse_response_type(_: agent_transport::HttpSseResponse) {}
fn assert_sse_event_type(_: agent_transport::SseEvent) {}

#[test]
fn root_and_module_types_are_interchangeable() {
    let policy_from_root: RetryPolicy = RetryPolicy::default();
    let policy_from_module: agent_transport::http::RetryPolicy = policy_from_root.clone();
    assert_eq!(policy_from_root, policy_from_module);

    let err_from_root: TransportError = TransportError::Serialization;
    let err_from_module: agent_transport::http::TransportError = err_from_root;
    assert!(matches!(
        err_from_module,
        agent_transport::http::TransportError::Serialization
    ));
}

#[test]
fn root_reexports_allow_transport_construction() {
    let policy = RetryPolicy {
        max_attempts: 2,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(20),
        ..RetryPolicy::default()
    };

    let transport = HttpTransport::builder(reqwest::Client::new())
        .retry_policy(policy)
        .timeout(Duration::from_secs(2))
        .build();
    assert_transport_type(transport);

    let builder_from_module = agent_transport::http::HttpTransport::builder(reqwest::Client::new());
    assert_builder_type(builder_from_module);
}

#[test]
fn root_reexports_expose_http_json_response_type() {
    let _ = std::mem::size_of::<agent_transport::HttpJsonResponse>();
    let _ = std::mem::size_of::<agent_transport::http::HttpJsonResponse>();
    let _ = std::mem::size_of::<HttpSseResponse>();
    let _ = std::mem::size_of::<agent_transport::http::HttpSseResponse>();
    let _ = std::mem::size_of::<SseEvent>();
    let _ = std::mem::size_of::<agent_transport::http::SseEvent>();

    let _assert_fn: fn(agent_transport::http::HttpJsonResponse) = assert_response_type;
    let _assert_sse_response_fn: fn(agent_transport::http::HttpSseResponse) =
        assert_sse_response_type;
    let _assert_sse_event_fn: fn(agent_transport::http::SseEvent) = assert_sse_event_type;
}
