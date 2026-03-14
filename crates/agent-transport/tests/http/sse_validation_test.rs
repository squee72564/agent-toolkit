use std::io::ErrorKind;

use agent_core::types::AuthStyle;
use agent_transport::{RetryPolicy, SseLimits, TransportError};
use reqwest::StatusCode;
use serde_json::json;

use crate::support::http_server::{
    ScriptedBody, ScriptedResponse, await_server, spawn_scripted_server,
};
use crate::support::{ExampleBody, TestResult, default_platform, default_transport};

#[tokio::test]
async fn post_sse_rejects_invalid_retry_field() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::Chunks(vec!["retry: nope\n\n".to_string()]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let mut response = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            None,
        )
        .await?;

    let error = response
        .stream
        .next_event()
        .await
        .expect_err("invalid retry field should fail");
    assert!(matches!(error, TransportError::SseParse(_)));

    if let Err(error) = await_server(handle).await {
        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
    }
    Ok(())
}

#[tokio::test]
async fn post_sse_rejects_non_sse_content_type() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        delay_before_headers: None,
        body: ScriptedBody::Fixed(json!({"ok": true}).to_string()),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy {
        max_attempts: 2,
        initial_backoff: std::time::Duration::from_millis(1),
        max_backoff: std::time::Duration::from_millis(1),
        ..RetryPolicy::default()
    });
    let error = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            None,
        )
        .await
        .expect_err("non-SSE response should fail");

    match error {
        TransportError::ContentTypeMismatch {
            expected,
            actual,
            head,
        } => {
            assert_eq!(expected, "text/event-stream");
            assert_eq!(actual.as_deref(), Some("application/json"));
            assert_eq!(head.status, StatusCode::OK);
        }
        other => panic!("unexpected error: {other:?}"),
    }

    if let Err(error) = await_server(handle).await {
        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
    }
    Ok(())
}

#[tokio::test]
async fn post_sse_accepts_content_type_with_charset() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![(
            "content-type".to_string(),
            "text/event-stream; charset=utf-8".to_string(),
        )],
        delay_before_headers: None,
        body: ScriptedBody::Chunks(vec!["data: ready\n\n".to_string()]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let mut response = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            None,
        )
        .await?;

    let event = response.stream.next_event().await?.expect("SSE event");
    assert_eq!(event.data, "ready");

    if let Err(error) = await_server(handle).await {
        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
    }
    Ok(())
}

#[tokio::test]
async fn post_sse_rejects_oversized_line() -> TestResult {
    let oversized_line = format!("data: {}\n", "x".repeat(64));
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::Chunks(vec![oversized_line]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = agent_transport::HttpTransport::builder(reqwest::Client::new())
        .retry_policy(RetryPolicy::default())
        .request_timeout(std::time::Duration::from_secs(2))
        .stream_timeout(std::time::Duration::from_secs(2))
        .sse_limits(SseLimits {
            max_line_bytes: 16,
            max_event_bytes: 1024,
            max_buffer_bytes: 1024,
        })
        .build();

    let mut response = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            None,
        )
        .await?;

    let error = response
        .stream
        .next_event()
        .await
        .expect_err("oversized line should fail");
    assert!(matches!(
        error,
        TransportError::SseLimit {
            kind: "SSE line",
            ..
        }
    ));

    await_server(handle).await?;
    Ok(())
}

#[tokio::test]
async fn post_sse_rejects_invalid_utf8() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::RawChunks(vec![vec![
            b'd', b'a', b't', b'a', b':', b' ', 0xff, b'\n', b'\n',
        ]]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let mut response = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            None,
        )
        .await?;

    let error = response
        .stream
        .next_event()
        .await
        .expect_err("invalid UTF-8 should fail");
    assert!(matches!(error, TransportError::SseParse(_)));

    await_server(handle).await?;
    Ok(())
}

#[tokio::test]
async fn post_sse_reports_disconnect_with_partial_frame() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::RawChunksThenDisconnect(vec![b"data: partial".to_vec()]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let mut response = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            None,
        )
        .await?;

    let error = response
        .stream
        .next_event()
        .await
        .expect_err("partial frame should fail");
    assert!(matches!(error, TransportError::StreamTerminated { .. }));

    await_server(handle).await?;
    Ok(())
}
