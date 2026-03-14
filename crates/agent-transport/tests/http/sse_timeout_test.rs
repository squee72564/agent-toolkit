use std::io::ErrorKind;
use std::time::Duration;

use agent_core::types::AuthStyle;
use agent_transport::{
    HttpRequestBody, HttpRequestOptions, HttpResponse, RetryPolicy, TimeoutStage, TransportError,
    TransportExecutionInput, TransportResponseFraming,
};
use reqwest::StatusCode;

use crate::support::http_server::{
    ScriptedBody, ScriptedResponse, await_server, spawn_scripted_server,
};
use crate::support::{
    ExampleBody, TestResult, default_platform, default_resolved_transport, default_transport,
};

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
            None,
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
async fn send_sse_times_out_waiting_for_first_byte() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::TimedChunks(vec![(
            Duration::from_millis(100),
            "data: late\n\n".to_string(),
        )]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let platform = default_platform(AuthStyle::None);
    let error = transport
        .send(TransportExecutionInput {
            platform: &platform,
            auth: None,
            method: reqwest::Method::POST,
            url: &format!("{base_url}/stream"),
            body: HttpRequestBody::Json(serde_json::to_vec(&ExampleBody { msg: "hello" })?.into()),
            response_framing: TransportResponseFraming::Sse,
            options: HttpRequestOptions::default()
                .with_accept(reqwest::header::HeaderValue::from_static(
                    "text/event-stream",
                ))
                .with_expected_content_type("text/event-stream"),
            transport: agent_core::ResolvedTransportOptions {
                timeouts: agent_core::TransportTimeoutOverrides {
                    request_timeout: Some(Duration::from_secs(2)),
                    stream_setup_timeout: Some(Duration::from_secs(2)),
                    stream_idle_timeout: Some(Duration::from_millis(20)),
                },
                ..default_resolved_transport(RetryPolicy::default())
            },
            provider_headers: reqwest::header::HeaderMap::new(),
        })
        .await
        .expect_err("first byte timeout should fail");

    assert!(matches!(
        error,
        TransportError::Timeout {
            stage: TimeoutStage::FirstByte
        }
    ));

    if let Err(error) = await_server(handle).await {
        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
    }
    Ok(())
}

#[tokio::test]
async fn send_sse_times_out_when_stream_goes_idle_after_first_event() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: None,
        body: ScriptedBody::TimedChunksThenDisconnect(vec![
            (Duration::from_millis(0), "data: partial\n\n".to_string()),
            (Duration::from_millis(100), "data: late\n\n".to_string()),
        ]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = default_transport(RetryPolicy::default());
    let platform = default_platform(AuthStyle::None);
    let mut response = match transport
        .send(TransportExecutionInput {
            platform: &platform,
            auth: None,
            method: reqwest::Method::POST,
            url: &format!("{base_url}/stream"),
            body: HttpRequestBody::Json(serde_json::to_vec(&ExampleBody { msg: "hello" })?.into()),
            response_framing: TransportResponseFraming::Sse,
            options: HttpRequestOptions::default()
                .with_accept(reqwest::header::HeaderValue::from_static(
                    "text/event-stream",
                ))
                .with_expected_content_type("text/event-stream"),
            transport: agent_core::ResolvedTransportOptions {
                timeouts: agent_core::TransportTimeoutOverrides {
                    request_timeout: Some(Duration::from_secs(2)),
                    stream_setup_timeout: Some(Duration::from_secs(2)),
                    stream_idle_timeout: Some(Duration::from_millis(20)),
                },
                ..default_resolved_transport(RetryPolicy::default())
            },
            provider_headers: reqwest::header::HeaderMap::new(),
        })
        .await?
    {
        HttpResponse::Sse(response) => *response,
        other => panic!("expected sse response, got {other:?}"),
    };

    let event = response.stream.next_event().await?.expect("first event");
    assert_eq!(event.data, "partial");

    let error = response
        .stream
        .next_event()
        .await
        .expect_err("idle timeout should fail");
    assert!(matches!(
        error,
        TransportError::Timeout {
            stage: TimeoutStage::StreamIdle
        }
    ));

    if let Err(error) = await_server(handle).await {
        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
    }
    Ok(())
}

#[tokio::test]
async fn send_sse_request_options_override_setup_timeout() -> TestResult {
    let responses = vec![ScriptedResponse {
        status: StatusCode::OK,
        headers: vec![],
        delay_before_headers: Some(Duration::from_millis(40)),
        body: ScriptedBody::Chunks(vec!["data: ready\n\n".to_string()]),
    }];
    let (base_url, _recorded, handle) = spawn_scripted_server(responses).await?;

    let transport = agent_transport::HttpTransport::builder(reqwest::Client::new())
        .retry_policy(RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        })
        .stream_timeout(Duration::from_millis(200))
        .build();
    let platform = default_platform(AuthStyle::None);
    let error = transport
        .send(TransportExecutionInput {
            platform: &platform,
            auth: None,
            method: reqwest::Method::POST,
            url: &format!("{base_url}/stream"),
            body: HttpRequestBody::Json(serde_json::to_vec(&ExampleBody { msg: "hello" })?.into()),
            response_framing: TransportResponseFraming::Sse,
            options: HttpRequestOptions::default()
                .with_accept(reqwest::header::HeaderValue::from_static(
                    "text/event-stream",
                ))
                .with_expected_content_type("text/event-stream"),
            transport: agent_core::ResolvedTransportOptions {
                timeouts: agent_core::TransportTimeoutOverrides {
                    request_timeout: Some(Duration::from_secs(30)),
                    stream_setup_timeout: Some(Duration::from_millis(10)),
                    stream_idle_timeout: Some(Duration::from_millis(200)),
                },
                ..default_resolved_transport(RetryPolicy {
                    max_attempts: 1,
                    ..RetryPolicy::default()
                })
            },
            provider_headers: reqwest::header::HeaderMap::new(),
        })
        .await
        .expect_err("request-scoped setup timeout should win");

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
