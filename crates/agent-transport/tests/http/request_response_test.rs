use std::collections::BTreeMap;
use std::time::Duration;

use agent_core::types::{AdapterContext, AuthStyle};
use agent_transport::RetryPolicy;
use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::support::http_server::{
    ScriptedBody, ScriptedResponse, await_server, captured_requests, spawn_scripted_server,
};
use crate::support::{ExampleBody, TestResult, default_platform, default_transport, empty_context};

#[tokio::test]
async fn post_json_value_preserves_non_success_status_and_extracts_request_id() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::BAD_REQUEST,
        headers: vec![("x-trace-id".to_string(), "trace-42".to_string())],
        body: ScriptedBody::Fixed(json!({"error": "bad request"}).to_string()),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let mut platform = default_platform(AuthStyle::None);
    platform.base_url = base_url.clone();

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.request_id_header".to_string(),
        "x-trace-id".to_string(),
    );
    metadata.insert(
        "transport.header.x-custom".to_string(),
        "custom".to_string(),
    );
    let ctx = AdapterContext {
        metadata,
        auth_token: None,
    };

    let transport = default_transport(RetryPolicy::default());
    let response = transport
        .post_json_value(
            &platform,
            &format!("{base_url}/v1/test"),
            &ExampleBody { msg: "hello" },
            &ctx,
        )
        .await?;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.request_id.as_deref(), Some("trace-42"));
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
            body: ScriptedBody::Fixed(json!({"error": "try again"}).to_string()),
        },
        ScriptedResponse {
            status: StatusCode::OK,
            headers: vec![],
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
            &empty_context(),
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
