use agent_core::types::AuthStyle;
use agent_transport::{HttpRequestBody, RetryPolicy, TransportRequestInput};
use reqwest::StatusCode;

use crate::support::http_server::{
    ScriptedBody, ScriptedResponse, await_server, captured_requests, spawn_scripted_server,
};
use crate::support::{TestResult, default_platform, default_resolved_transport, default_transport};

#[tokio::test]
async fn get_sse_supports_bodyless_streams() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::Chunks(vec!["data: ready\n\n".to_string()]),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let mut response = transport
        .get_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/events"),
            None,
        )
        .await?;

    let event = response.stream.next_event().await?.expect("SSE event");
    assert_eq!(event.data, "ready");
    assert!(response.stream.next_event().await?.is_none());

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "GET");
    assert_eq!(captured[0].path, "/events");
    assert!(captured[0].body.is_empty());
    assert_eq!(
        captured[0].headers.get("accept").map(String::as_str),
        Some("text/event-stream")
    );
    Ok(())
}

#[tokio::test]
async fn send_sse_request_supports_raw_bytes_body() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::Chunks(vec!["data: binary-ready\n\n".to_string()]),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let platform = default_platform(AuthStyle::None);
    let mut response = transport
        .send_sse_request(
            &platform,
            TransportRequestInput {
                method: reqwest::Method::POST,
                url: &format!("{base_url}/raw-stream"),
                body: HttpRequestBody::Bytes {
                    content_type: Some(reqwest::header::HeaderValue::from_static(
                        "application/octet-stream",
                    )),
                    body: bytes::Bytes::from_static(b"\x01\x02"),
                },
                auth: None,
                options: agent_transport::HttpRequestOptions::default(),
                transport: default_resolved_transport(RetryPolicy::default()),
            },
        )
        .await?;

    let event = response.stream.next_event().await?.expect("SSE event");
    assert_eq!(event.data, "binary-ready");
    assert!(response.stream.next_event().await?.is_none());

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].path, "/raw-stream");
    assert_eq!(captured[0].body, vec![1, 2]);
    assert_eq!(
        captured[0].headers.get("content-type").map(String::as_str),
        Some("application/octet-stream")
    );
    assert_eq!(
        captured[0].headers.get("accept").map(String::as_str),
        Some("text/event-stream")
    );
    Ok(())
}
