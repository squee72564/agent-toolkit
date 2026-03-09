use std::time::Duration;

use agent_transport::{
    HttpBytesResponse, HttpRequestBody, HttpRequestOptions, HttpResponse, HttpResponseHead,
    HttpResponseMode, HttpSseResponse, HttpTransport, HttpTransportBuilder, RetryPolicy, SseEvent,
    SseLimits, StreamTerminationReason, TimeoutStage, TransportError,
};

fn assert_builder_type(_: HttpTransportBuilder) {}
fn assert_transport_type(_: HttpTransport) {}
fn assert_response_type(_: agent_transport::HttpJsonResponse) {}
fn assert_bytes_response_type(_: agent_transport::HttpBytesResponse) {}
fn assert_sse_response_type(_: agent_transport::HttpSseResponse) {}
fn assert_sse_event_type(_: agent_transport::SseEvent) {}

#[test]
fn root_and_module_types_are_interchangeable() {
    let policy_from_root: RetryPolicy = RetryPolicy::default();
    let policy_from_module: agent_transport::http::RetryPolicy = policy_from_root.clone();
    assert_eq!(policy_from_root, policy_from_module);

    let err_from_root: TransportError = TransportError::Timeout {
        stage: TimeoutStage::Request,
    };
    let err_from_module: agent_transport::http::TransportError = err_from_root;
    assert!(matches!(
        err_from_module,
        agent_transport::http::TransportError::Timeout {
            stage: agent_transport::http::TimeoutStage::Request
        }
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
        .request_timeout(Duration::from_secs(2))
        .stream_timeout(Duration::from_secs(2))
        .sse_limits(SseLimits::default())
        .build();
    assert_transport_type(transport);

    let builder_from_module = agent_transport::http::HttpTransport::builder(reqwest::Client::new());
    assert_builder_type(builder_from_module);
}

#[test]
fn root_reexports_expose_http_json_response_type() {
    let _ = std::mem::size_of::<HttpResponseHead>();
    let _ = std::mem::size_of::<agent_transport::http::HttpResponseHead>();
    let _ = std::mem::size_of::<agent_transport::HttpJsonResponse>();
    let _ = std::mem::size_of::<agent_transport::http::HttpJsonResponse>();
    let _ = std::mem::size_of::<HttpBytesResponse>();
    let _ = std::mem::size_of::<agent_transport::http::HttpBytesResponse>();
    let _ = std::mem::size_of::<HttpSseResponse>();
    let _ = std::mem::size_of::<agent_transport::http::HttpSseResponse>();
    let _ = std::mem::size_of::<SseEvent>();
    let _ = std::mem::size_of::<agent_transport::http::SseEvent>();
    let _ = std::mem::size_of::<HttpRequestBody>();
    let _ = std::mem::size_of::<agent_transport::http::HttpRequestBody>();
    let _ = std::mem::size_of::<HttpRequestOptions>();
    let _ = std::mem::size_of::<agent_transport::http::HttpRequestOptions>();
    let _ = std::mem::size_of::<HttpResponse>();
    let _ = std::mem::size_of::<agent_transport::http::HttpResponse>();
    let _ = std::mem::size_of::<HttpResponseMode>();
    let _ = std::mem::size_of::<agent_transport::http::HttpResponseMode>();
    let _ = std::mem::size_of::<StreamTerminationReason>();
    let _ = std::mem::size_of::<agent_transport::http::StreamTerminationReason>();

    let _assert_fn: fn(agent_transport::http::HttpJsonResponse) = assert_response_type;
    let _assert_bytes_fn: fn(agent_transport::http::HttpBytesResponse) = assert_bytes_response_type;
    let _assert_sse_response_fn: fn(agent_transport::http::HttpSseResponse) =
        assert_sse_response_type;
    let _assert_sse_event_fn: fn(agent_transport::http::SseEvent) = assert_sse_event_type;
}
