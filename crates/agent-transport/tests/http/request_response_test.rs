use std::time::Duration;

use agent_core::types::AuthStyle;
use agent_transport::{
    HttpRequestBody, HttpRequestOptions, HttpResponse, HttpSendRequest, RetryPolicy,
    TransportRequestInput, TransportResponseFraming,
};
use bytes::Bytes;
use reqwest::StatusCode;
use reqwest::header::HeaderValue;
use serde_json::{Value, json};

use crate::support::http_server::{
    ScriptedBody, ScriptedResponse, await_server, captured_requests, spawn_scripted_server,
};
use crate::support::{
    ExampleBody, TestResult, default_platform, default_resolved_transport, default_transport,
};

#[tokio::test]
async fn post_json_value_preserves_non_success_status_and_extracts_request_id() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::BAD_REQUEST,
        headers: vec![("x-trace-id".to_string(), "trace-42".to_string())],
        delay_before_headers: None,
        body: ScriptedBody::Fixed(json!({"error": "bad request"}).to_string()),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let mut platform = default_platform(AuthStyle::None);
    platform.base_url = base_url.clone();

    let mut transport_options = default_resolved_transport(RetryPolicy::default());
    transport_options.request_id_header_override = Some("x-trace-id".to_string());
    transport_options
        .route_extra_headers
        .insert("x-custom".to_string(), "custom".to_string());

    let transport = default_transport(RetryPolicy::default());
    let response = transport
        .send(HttpSendRequest {
            platform: &platform,
            auth: None,
            method: reqwest::Method::POST,
            url: &format!("{base_url}/v1/test"),
            body: HttpRequestBody::Json(Bytes::from_static(br#"{"msg":"hello"}"#)),
            response_framing: TransportResponseFraming::Json,
            options: HttpRequestOptions::json_defaults().with_allow_error_status(true),
            transport: transport_options,
            provider_headers: reqwest::header::HeaderMap::new(),
        })
        .await?;

    let response = match response {
        HttpResponse::Json(response) => response,
        other => panic!("expected json response, got {other:?}"),
    };

    assert_eq!(response.head.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.head.request_id.as_deref(), Some("trace-42"));
    assert_eq!(response.body, json!({"error": "bad request"}));

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].path, "/v1/test");
    assert_eq!(
        captured[0].headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        captured[0].headers.get("x-custom").map(String::as_str),
        Some("custom")
    );
    assert!(!captured[0].headers.contains_key("x-trace-id"));

    let body: Value = serde_json::from_slice(&captured[0].body)?;
    assert_eq!(body, json!({"msg": "hello"}));
    Ok(())
}

#[tokio::test]
async fn get_json_retries_retryable_status_then_succeeds() -> TestResult {
    let responses = vec![
        ScriptedResponse {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: vec![],
            delay_before_headers: None,
            body: ScriptedBody::Fixed(json!({"error": "try again"}).to_string()),
        },
        ScriptedResponse {
            status: StatusCode::OK,
            headers: vec![],
            delay_before_headers: None,
            body: ScriptedBody::Fixed(json!({"ok": true}).to_string()),
        },
    ];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let policy = RetryPolicy {
        max_attempts: 2,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(1),
        ..RetryPolicy::default()
    };

    let transport = default_transport(policy);
    let result: Value = transport
        .get_json(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/retry"),
            None,
        )
        .await?;

    assert_eq!(result, json!({"ok": true}));

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 2);
    assert_eq!(captured[0].path, "/retry");
    assert_eq!(captured[1].path, "/retry");
    Ok(())
}

#[tokio::test]
async fn send_bytes_without_body_does_not_set_content_type() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![("content-type".to_string(), "text/plain".to_string())],
        delay_before_headers: None,
        body: ScriptedBody::Fixed("pong".to_string()),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let platform = default_platform(AuthStyle::None);
    let response = transport
        .send(HttpSendRequest {
            platform: &platform,
            auth: None,
            method: reqwest::Method::GET,
            url: &format!("{base_url}/ping"),
            body: HttpRequestBody::None,
            response_framing: TransportResponseFraming::Bytes,
            options: HttpRequestOptions::default().with_expected_content_type("text/plain"),
            transport: default_resolved_transport(RetryPolicy::default()),
            provider_headers: reqwest::header::HeaderMap::new(),
        })
        .await?;

    match response {
        HttpResponse::Bytes(response) => assert_eq!(response.body, Bytes::from_static(b"pong")),
        other => panic!("expected bytes response, got {other:?}"),
    }

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "GET");
    assert_eq!(captured[0].path, "/ping");
    assert!(!captured[0].headers.contains_key("content-type"));
    assert!(captured[0].body.is_empty());
    Ok(())
}

#[tokio::test]
async fn send_bytes_preserves_raw_payload_and_explicit_content_type() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![(
            "content-type".to_string(),
            "application/octet-stream".to_string(),
        )],
        delay_before_headers: None,
        body: ScriptedBody::RawChunks(vec![b"ok".to_vec()]),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let platform = default_platform(AuthStyle::None);
    let response = transport
        .send(HttpSendRequest {
            platform: &platform,
            auth: None,
            method: reqwest::Method::POST,
            url: &format!("{base_url}/upload"),
            body: HttpRequestBody::Bytes {
                content_type: Some(HeaderValue::from_static("application/octet-stream")),
                body: Bytes::from_static(b"\x00\x01\x02"),
            },
            response_framing: TransportResponseFraming::Bytes,
            options: HttpRequestOptions::default()
                .with_accept(HeaderValue::from_static("application/octet-stream"))
                .with_expected_content_type("application/octet-stream"),
            transport: default_resolved_transport(RetryPolicy::default()),
            provider_headers: reqwest::header::HeaderMap::new(),
        })
        .await?;

    match response {
        HttpResponse::Bytes(response) => assert_eq!(response.body, Bytes::from_static(b"ok")),
        other => panic!("expected bytes response, got {other:?}"),
    }

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].path, "/upload");
    assert_eq!(
        captured[0].headers.get("content-type").map(String::as_str),
        Some("application/octet-stream")
    );
    assert_eq!(
        captured[0].headers.get("accept").map(String::as_str),
        Some("application/octet-stream")
    );
    assert_eq!(captured[0].body, vec![0, 1, 2]);
    Ok(())
}

#[tokio::test]
async fn send_json_response_supports_non_post_methods_and_preserves_status() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::ACCEPTED,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::Fixed(json!({"accepted": true}).to_string()),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let platform = default_platform(AuthStyle::None);
    let response = transport
        .send_json_response(
            &platform,
            reqwest::Method::PUT,
            &format!("{base_url}/v1/update"),
            &ExampleBody { msg: "hello" },
            None,
        )
        .await?;

    assert_eq!(response.head.status, StatusCode::ACCEPTED);
    assert_eq!(response.body, json!({"accepted": true}));

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "PUT");
    assert_eq!(captured[0].path, "/v1/update");
    Ok(())
}

#[tokio::test]
async fn send_json_mode_can_preserve_error_status_when_opted_in() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::UNAUTHORIZED,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        delay_before_headers: None,
        body: ScriptedBody::Fixed(json!({"error": {"message": "bad key"}}).to_string()),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let platform = default_platform(AuthStyle::None);
    let response = transport
        .send(HttpSendRequest {
            platform: &platform,
            auth: None,
            method: reqwest::Method::POST,
            url: &format!("{base_url}/v1/test"),
            body: HttpRequestBody::Json(Bytes::from_static(br#"{"msg":"hello"}"#)),
            response_framing: TransportResponseFraming::Json,
            options: HttpRequestOptions::json_defaults().with_allow_error_status(true),
            transport: default_resolved_transport(RetryPolicy::default()),
            provider_headers: reqwest::header::HeaderMap::new(),
        })
        .await?;

    match response {
        HttpResponse::Json(response) => {
            assert_eq!(response.head.status, StatusCode::UNAUTHORIZED);
            assert_eq!(response.body, json!({"error": {"message": "bad key"}}));
        }
        other => panic!("expected json response, got {other:?}"),
    }

    await_server(handle).await?;
    Ok(())
}

#[tokio::test]
async fn send_bytes_request_returns_bytes_helper_result() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![(
            "content-type".to_string(),
            "application/octet-stream".to_string(),
        )],
        delay_before_headers: None,
        body: ScriptedBody::RawChunks(vec![b"done".to_vec()]),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let platform = default_platform(AuthStyle::None);
    let response = transport
        .send_bytes_request(
            &platform,
            TransportRequestInput {
                method: reqwest::Method::POST,
                url: &format!("{base_url}/binary"),
                body: HttpRequestBody::Bytes {
                    content_type: Some(HeaderValue::from_static("application/octet-stream")),
                    body: Bytes::from_static(b"\xaa\xbb"),
                },
                auth: None,
                options: HttpRequestOptions::default()
                    .with_accept(HeaderValue::from_static("application/octet-stream"))
                    .with_expected_content_type("application/octet-stream"),
                transport: default_resolved_transport(RetryPolicy::default()),
            },
        )
        .await?;

    assert_eq!(response.body, Bytes::from_static(b"done"));

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].path, "/binary");
    assert_eq!(captured[0].body, vec![0xaa, 0xbb]);
    Ok(())
}
