use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::time::Duration;

use agent_core::types::AuthStyle;
use agent_transport::{RetryPolicy, SseLimits, TimeoutStage, TransportError};
use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::support::http_server::{
    ScriptedBody, ScriptedResponse, await_server, captured_requests, spawn_scripted_server,
};
use crate::support::{ExampleBody, TestResult, default_platform, default_transport, empty_context};

#[tokio::test]
async fn post_sse_streams_events_and_preserves_metadata() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![("x-trace-id".to_string(), "trace-sse-1".to_string())],
        delay_before_headers: None,
        body: ScriptedBody::Chunks(vec![
            ": keep-alive\n".to_string(),
            "event: response.output_text.delta\n".to_string(),
            "id: event-1\n".to_string(),
            "retry: 1500\n".to_string(),
            "data: hello\n".to_string(),
            "data: world\n\n".to_string(),
            "data: [DONE]\n\n".to_string(),
        ]),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let mut platform = default_platform(AuthStyle::None);
    platform.base_url = base_url.clone();

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "transport.request_id_header".to_string(),
        "x-trace-id".to_string(),
    );

    let transport = default_transport(RetryPolicy::default());
    let mut response = transport
        .post_sse(
            &platform,
            &format!("{base_url}/v1/stream"),
            &ExampleBody { msg: "hello" },
            &agent_core::AdapterContext {
                metadata,
                auth_token: None,
            },
        )
        .await?;

    assert_eq!(response.head.status, StatusCode::OK);
    assert_eq!(response.head.request_id.as_deref(), Some("trace-sse-1"));
    assert_eq!(
        response
            .head
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );

    let first = response
        .stream
        .next_event()
        .await?
        .expect("first SSE event");
    assert_eq!(first.event.as_deref(), Some("response.output_text.delta"));
    assert_eq!(first.id.as_deref(), Some("event-1"));
    assert_eq!(first.retry, Some(1500));
    assert_eq!(first.data, "hello\nworld");

    let second = response.stream.next_event().await?.expect("done event");
    assert_eq!(second.event, None);
    assert_eq!(second.id, None);
    assert_eq!(second.retry, None);
    assert_eq!(second.data, "[DONE]");

    assert!(response.stream.next_event().await?.is_none());

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].path, "/v1/stream");
    assert_eq!(
        captured[0].headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        captured[0].headers.get("accept").map(String::as_str),
        Some("text/event-stream")
    );

    let body: Value = serde_json::from_slice(&captured[0].body)?;
    assert_eq!(body, json!({"msg": "hello"}));
    Ok(())
}

#[tokio::test]
async fn post_sse_retries_retryable_status_before_stream_start() -> TestResult {
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
            body: ScriptedBody::Chunks(vec!["data: ready\n\n".to_string()]),
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
    let mut response = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/retry"),
            &ExampleBody { msg: "hello" },
            &empty_context(),
        )
        .await?;

    let event = response.stream.next_event().await?.expect("SSE event");
    assert_eq!(event.data, "ready");
    assert!(response.stream.next_event().await?.is_none());

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 2);
    assert_eq!(captured[0].path, "/retry");
    assert_eq!(captured[1].path, "/retry");
    Ok(())
}

#[tokio::test]
async fn post_sse_does_not_retry_after_stream_has_started() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::ChunksThenDisconnect(vec!["data: partial\n\n".to_string()]),
    }];
    let (base_url, recorded, handle) = spawn_scripted_server(responses).await?;

    let policy = RetryPolicy {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(1),
        ..RetryPolicy::default()
    };

    let transport = default_transport(policy);
    let mut response = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            &empty_context(),
        )
        .await?;

    let first = response.stream.next_event().await?.expect("first event");
    assert_eq!(first.data, "partial");

    let error = response
        .stream
        .next_event()
        .await
        .expect_err("mid-stream disconnect");
    assert!(matches!(
        error,
        TransportError::StreamTerminated { .. } | TransportError::SseParse(_)
    ));

    await_server(handle).await?;

    let captured = captured_requests(&recorded)?;
    assert_eq!(captured.len(), 1);
    Ok(())
}

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
            &empty_context(),
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

    let transport = default_transport(RetryPolicy::default());
    let error = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            &empty_context(),
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
            &empty_context(),
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
async fn post_sse_times_out_before_response_headers() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: Some(Duration::from_millis(100)),
        body: ScriptedBody::Chunks(vec!["data: late\n\n".to_string()]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = agent_transport::HttpTransport::builder(reqwest::Client::new())
        .retry_policy(RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        })
        .request_timeout(Duration::from_secs(2))
        .stream_timeout(Duration::from_millis(20))
        .build();

    let error = transport
        .post_sse(
            &default_platform(AuthStyle::None),
            &format!("{base_url}/stream"),
            &ExampleBody { msg: "hello" },
            &empty_context(),
        )
        .await
        .expect_err("setup timeout should fail");

    assert!(matches!(
        error,
        TransportError::Timeout {
            stage: TimeoutStage::StreamSetup
        }
    ));

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
        .request_timeout(Duration::from_secs(2))
        .stream_timeout(Duration::from_secs(2))
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
            &empty_context(),
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
            &empty_context(),
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
            &empty_context(),
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
