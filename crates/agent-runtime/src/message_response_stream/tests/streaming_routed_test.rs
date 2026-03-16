use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;

use agent_core::{CanonicalStreamEvent, ProviderInstanceId, ProviderKind, ResponseMode};

use crate::agent_toolkit::AgentToolkit;
use crate::message::MessageCreateInput;
use crate::provider::ProviderConfig;
use crate::routing::{
    AttemptDisposition, FallbackPolicy, FallbackRule, PlanningRejectionPolicy, Route,
    RoutePlanningFailureReason, SkipReason, Target,
};
use crate::{ExecutionOptions, RuntimeErrorKind};

use crate::test::stream_test_fixtures::spawn_sse_stub;
use crate::test::streaming_test_fixtures::{RecordingObserver, as_attempt_skipped, event_names};
use crate::test::{
    executed_failure_meta, route_planning_failure, test_provider_client,
    test_provider_client_with_base_url, test_provider_client_with_streaming_support,
};

#[tokio::test]
async fn routed_streaming_happy_path_finishes_with_response_meta() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hello from route\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openai_default())),
        )
        .await
        .expect("stream should open");

    let first = stream
        .next()
        .await
        .expect("stream should yield")
        .expect("stream item should succeed");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.attempts.len(), 1);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("hello from route")]
    );
}

#[tokio::test]
async fn routed_streaming_retries_next_target_when_initial_stream_open_fails() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"openai/gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"fallback stream\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"openai/gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("test-key")
                .with_base_url("http://127.0.0.1:1")
                .with_default_model("gpt-5-mini"),
        )
        .with_openrouter(
            ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("openai/gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(ProviderInstanceId::openrouter_default()))
                .with_fallback_policy(
                    FallbackPolicy::new()
                        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Transport)),
                ),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("fallback stream should open");

    let first = stream
        .next()
        .await
        .expect("stream should yield")
        .expect("stream item should succeed");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(
        completion.meta.selected_provider_kind,
        ProviderKind::OpenRouter
    );
    assert_eq!(completion.meta.attempts.len(), 2);
    assert_eq!(
        completion.meta.attempts[0].provider_kind,
        ProviderKind::OpenAi
    );
    assert!(matches!(
        completion.meta.attempts[0].disposition,
        AttemptDisposition::Failed { .. }
    ));
    assert_eq!(
        completion.meta.attempts[1].provider_kind,
        ProviderKind::OpenRouter
    );
    assert!(matches!(
        completion.meta.attempts[1].disposition,
        AttemptDisposition::Succeeded { .. }
    ));
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("fallback stream")]
    );
}

#[tokio::test]
async fn routed_streaming_allows_fallback_after_raw_envelope_without_canonical_events() {
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"recovered after raw frame\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let primary_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "data: keep-alive before any canonical event\n\n",
            "data: {\"missing\":\"type\"}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openrouter(
            ProviderConfig::new("test-key")
                .with_base_url(primary_url)
                .with_default_model("openai/gpt-5-mini"),
        )
        .with_openai(
            ProviderConfig::new("test-key")
                .with_base_url(fallback_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openrouter_default()))
                .with_fallback(Target::new(ProviderInstanceId::openai_default()))
                .with_fallback_policy(FallbackPolicy::new().with_rule(
                    FallbackRule::retry_on_kind(RuntimeErrorKind::ProtocolViolation),
                )),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open");

    let first = stream
        .next()
        .await
        .expect("stream should yield raw setup envelope")
        .expect("setup envelope should not fail");
    assert!(first.canonical.is_empty());

    let second = stream
        .next()
        .await
        .expect("fallback stream should yield")
        .expect("fallback stream item should succeed");
    assert!(!second.canonical.is_empty());

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.attempts.len(), 2);
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("recovered after raw frame")]
    );
}

#[tokio::test]
async fn routed_streaming_explicit_task_api_uses_route_and_execution_options() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"explicit route stream\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("test-key")
                .with_base_url(base_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let task = MessageCreateInput::user("hello explicit route")
        .into_task_request()
        .expect("task request should build");
    let route =
        Route::to(Target::new(ProviderInstanceId::openai_default()).with_model("gpt-5-mini"));
    let execution = ExecutionOptions {
        response_mode: ResponseMode::Streaming,
        ..ExecutionOptions::default()
    };

    let mut stream = toolkit
        .streaming()
        .execute(task, route, execution)
        .await
        .expect("explicit routed stream should open");

    let first = stream
        .next()
        .await
        .expect("stream should yield")
        .expect("stream item should succeed");
    assert_eq!(first.raw.sequence, 1);

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(completion.meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("explicit route stream")]
    );
}

#[tokio::test]
async fn routed_streaming_does_not_fallback_after_first_canonical_event() {
    let primary_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-mini\",\"error\":{\"message\":\"stream failed after commit\"},\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\",\"model\":\"openai/gpt-5-mini\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_2\",\"delta\":\"should never be used\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_2\",\"model\":\"openai/gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let observer = Arc::new(RecordingObserver::new());
    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new("test-key")
                .with_base_url(primary_url)
                .with_default_model("gpt-5-mini"),
        )
        .with_openrouter(
            ProviderConfig::new("test-key")
                .with_base_url(fallback_url)
                .with_default_model("openai/gpt-5-mini"),
        )
        .observer(observer.clone())
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(ProviderInstanceId::openrouter_default()))
                .with_fallback_policy(
                    FallbackPolicy::new()
                        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Upstream)),
                ),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open");

    let first = stream
        .next()
        .await
        .expect("stream should yield canonical event")
        .expect("first canonical event should succeed");
    assert!(!first.canonical.is_empty());

    let terminal = stream
        .next()
        .await
        .expect("stream should surface provider terminal event")
        .expect("committed stream should stay on the active attempt");
    assert_eq!(
        terminal.canonical,
        vec![CanonicalStreamEvent::Failed {
            message: "stream failed after commit".to_string(),
        }]
    );

    let finish_error = stream
        .finish()
        .await
        .expect_err("finish should return the committed-stream error");
    assert_eq!(finish_error.kind, RuntimeErrorKind::Upstream);
    assert!(finish_error.message.contains("stream failed after commit"));
    let failure_meta = executed_failure_meta(&finish_error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(failure_meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(failure_meta.selected_model, "gpt-5-mini");
    assert_eq!(failure_meta.status_code, Some(200));
    assert_eq!(failure_meta.request_id.as_deref(), Some("req_sse"));
    assert_eq!(failure_meta.attempts.len(), 1);
    assert!(matches!(
        failure_meta.attempts[0].disposition,
        AttemptDisposition::Failed {
            error_kind: RuntimeErrorKind::Upstream,
            ..
        }
    ));

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec![
            "request_start",
            "attempt_start",
            "attempt_failure",
            "request_end"
        ]
    );
}

#[tokio::test]
async fn routed_streaming_terminal_error_carries_ordered_attempt_history() {
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_2\",\"model\":\"gpt-5-mini\",\"error\":{\"message\":\"fallback stream failed after commit\"},\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let primary_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "data: keep-alive before any canonical event\n\n",
            "data: {\"missing\":\"type\"}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit::builder()
        .with_openrouter(
            ProviderConfig::new("test-key")
                .with_base_url(primary_url)
                .with_default_model("openai/gpt-5-mini"),
        )
        .with_openai(
            ProviderConfig::new("test-key")
                .with_base_url(fallback_url)
                .with_default_model("gpt-5-mini"),
        )
        .build()
        .expect("toolkit should build");

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openrouter_default()))
                .with_fallback(Target::new(ProviderInstanceId::openai_default()))
                .with_fallback_policy(FallbackPolicy::new().with_rule(
                    FallbackRule::retry_on_kind(RuntimeErrorKind::ProtocolViolation),
                )),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should open");

    let first = stream
        .next()
        .await
        .expect("primary stream should yield")
        .expect("primary setup envelope should not fail");
    assert!(first.canonical.is_empty());

    let second = stream
        .next()
        .await
        .expect("fallback stream should yield")
        .expect("fallback canonical event should succeed");
    assert!(!second.canonical.is_empty());

    let terminal = stream
        .next()
        .await
        .expect("fallback stream should surface terminal event")
        .expect("terminal envelope should stay on fallback attempt");
    assert_eq!(
        terminal.canonical,
        vec![CanonicalStreamEvent::Failed {
            message: "fallback stream failed after commit".to_string(),
        }]
    );

    let finish_error = stream
        .finish()
        .await
        .expect_err("finish should return the fallback attempt error");
    let failure_meta = executed_failure_meta(&finish_error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(failure_meta.selected_provider_kind, ProviderKind::OpenAi);
    assert_eq!(failure_meta.selected_model, "gpt-5-mini");
    assert_eq!(failure_meta.attempts.len(), 2);
    assert_eq!(
        failure_meta.attempts[0].provider_instance,
        ProviderInstanceId::openrouter_default()
    );
    assert!(matches!(
        failure_meta.attempts[0].disposition,
        AttemptDisposition::Failed {
            error_kind: RuntimeErrorKind::ProtocolViolation,
            ..
        }
    ));
    assert_eq!(
        failure_meta.attempts[1].provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert!(matches!(
        failure_meta.attempts[1].disposition,
        AttemptDisposition::Failed {
            error_kind: RuntimeErrorKind::Upstream,
            ..
        }
    ));
}

#[tokio::test]
async fn routed_streaming_terminal_error_keeps_pre_open_failures_and_skips_before_open_success() {
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_3\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_3\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_3\",\"delta\":\"opened after retry\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_3\",\"model\":\"gpt-5-mini\",\"error\":{\"message\":\"final stream failed after retry\"},\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenRouter,
                    "http://127.0.0.1:1",
                    Some("openai/gpt-5-mini"),
                ),
            ),
            (
                ProviderInstanceId::openai_default(),
                test_provider_client_with_streaming_support(
                    ProviderKind::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                ProviderInstanceId::generic_openai_compatible_default(),
                test_provider_client_with_base_url(
                    ProviderKind::GenericOpenAiCompatible,
                    &fallback_url,
                    Some("gpt-5-mini"),
                ),
            ),
        ]),
        observer: None,
    };

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openrouter_default()))
                .with_fallback(Target::new(ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(
                    ProviderInstanceId::generic_openai_compatible_default(),
                ))
                .with_fallback_policy(
                    FallbackPolicy::new()
                        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Transport)),
                )
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("third target should open after a failed open and a skipped target");

    let first = stream
        .next()
        .await
        .expect("opened stream should yield")
        .expect("canonical event should succeed");
    assert!(!first.canonical.is_empty());

    let terminal = loop {
        let envelope = stream
            .next()
            .await
            .expect("stream should keep yielding until the terminal event")
            .expect("terminal envelopes should stay on the opened attempt");
        if matches!(
            envelope.canonical.as_slice(),
            [CanonicalStreamEvent::Failed { .. }]
        ) {
            break envelope;
        }
    };
    assert_eq!(
        terminal.canonical,
        vec![CanonicalStreamEvent::Failed {
            message: "final stream failed after retry".to_string(),
        }]
    );

    let finish_error = stream
        .finish()
        .await
        .expect_err("finish should return the committed terminal failure");
    let failure_meta = executed_failure_meta(&finish_error);
    assert_eq!(
        failure_meta.selected_provider_instance,
        ProviderInstanceId::generic_openai_compatible_default()
    );
    assert_eq!(
        failure_meta.selected_provider_kind,
        ProviderKind::GenericOpenAiCompatible
    );
    assert_eq!(failure_meta.attempts.len(), 3);
    assert_eq!(
        failure_meta.attempts[0].provider_instance,
        ProviderInstanceId::openrouter_default()
    );
    assert!(matches!(
        failure_meta.attempts[0].disposition,
        AttemptDisposition::Failed {
            error_kind: RuntimeErrorKind::Transport,
            ..
        }
    ));
    assert_eq!(
        failure_meta.attempts[1].provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert!(matches!(
        failure_meta.attempts[1].disposition,
        AttemptDisposition::Skipped {
            reason: SkipReason::StaticIncompatibility { .. },
        }
    ));
    assert_eq!(
        failure_meta.attempts[2].provider_instance,
        ProviderInstanceId::generic_openai_compatible_default()
    );
    assert!(matches!(
        failure_meta.attempts[2].disposition,
        AttemptDisposition::Failed {
            error_kind: RuntimeErrorKind::Upstream,
            ..
        }
    ));
}

#[tokio::test]
async fn routed_streaming_success_uses_typed_attempt_history_for_legacy_response_meta() {
    let fallback_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_4\",\"model\":\"gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_4\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_4\",\"delta\":\"opened after typed history\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_4\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_4\",\"model\":\"gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenRouter,
                    "http://127.0.0.1:1",
                    Some("openai/gpt-5-mini"),
                ),
            ),
            (
                ProviderInstanceId::openai_default(),
                test_provider_client_with_streaming_support(
                    ProviderKind::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                ProviderInstanceId::generic_openai_compatible_default(),
                test_provider_client_with_base_url(
                    ProviderKind::GenericOpenAiCompatible,
                    &fallback_url,
                    Some("gpt-5-mini"),
                ),
            ),
        ]),
        observer: None,
    };

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openrouter_default()))
                .with_fallback(Target::new(ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(
                    ProviderInstanceId::generic_openai_compatible_default(),
                ))
                .with_fallback_policy(
                    FallbackPolicy::new()
                        .with_rule(FallbackRule::retry_on_kind(RuntimeErrorKind::Transport)),
                )
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("third target should open after a failed open and a skipped target");

    let first = stream
        .next()
        .await
        .expect("opened stream should yield")
        .expect("canonical event should succeed");
    assert!(!first.canonical.is_empty());

    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(
        completion.meta.selected_provider_kind,
        ProviderKind::GenericOpenAiCompatible
    );
    assert_eq!(completion.meta.selected_model, "gpt-5-mini");
    assert_eq!(completion.meta.attempts.len(), 3);
    assert_eq!(
        completion.meta.attempts[0].provider_kind,
        ProviderKind::OpenRouter
    );
    assert!(matches!(
        completion.meta.attempts[0].disposition,
        AttemptDisposition::Failed { .. }
    ));
    assert_eq!(
        completion.meta.attempts[1].provider_kind,
        ProviderKind::OpenAi
    );
    assert!(matches!(
        completion.meta.attempts[1].disposition,
        AttemptDisposition::Skipped { .. }
    ));
    assert_eq!(
        completion.meta.attempts[2].provider_kind,
        ProviderKind::GenericOpenAiCompatible
    );
    assert!(matches!(
        completion.meta.attempts[2].disposition,
        AttemptDisposition::Succeeded { .. }
    ));
    assert_eq!(
        completion.response.output.content,
        vec![agent_core::ContentPart::text("opened after typed history")]
    );
}

#[tokio::test]
async fn routed_streaming_fail_fast_stops_on_planning_rejection_before_fallback() {
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::openai_default(),
                test_provider_client_with_streaming_support(
                    ProviderKind::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client(ProviderKind::OpenRouter),
            ),
        ]),
        observer: None,
    };

    let error = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(ProviderInstanceId::openrouter_default()))
                .with_planning_rejection_policy(PlanningRejectionPolicy::FailFast),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect_err("planning rejection must stop before fallback");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);
    let failure = route_planning_failure(&error);
    assert_eq!(
        failure.reason,
        RoutePlanningFailureReason::NoCompatibleAttempts
    );
    assert_eq!(failure.attempts.len(), 1);
    assert_eq!(failure.attempts[0].model, "gpt-5-mini");
    assert!(matches!(
        failure.attempts[0].disposition,
        AttemptDisposition::Skipped {
            reason: SkipReason::StaticIncompatibility { .. }
        }
    ));
}

#[tokio::test]
async fn routed_streaming_emits_attempt_skipped_without_execution_events_for_skipped_target() {
    let base_url = spawn_sse_stub(
        "text/event-stream",
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"openai/gpt-5-mini\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"item_id\":\"msg_1\",\"delta\":\"stream fallback success\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"openai/gpt-5-mini\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
        ),
    )
    .await;
    let observer = Arc::new(RecordingObserver::new());
    let toolkit = AgentToolkit {
        clients: HashMap::from([
            (
                ProviderInstanceId::openai_default(),
                test_provider_client_with_streaming_support(
                    ProviderKind::OpenAi,
                    Some("gpt-5-mini"),
                    false,
                ),
            ),
            (
                ProviderInstanceId::openrouter_default(),
                test_provider_client_with_base_url(
                    ProviderKind::OpenRouter,
                    &base_url,
                    Some("openai/gpt-5-mini"),
                ),
            ),
        ]),
        observer: Some(observer.clone()),
    };

    let mut stream = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openai_default()))
                .with_fallback(Target::new(ProviderInstanceId::openrouter_default()))
                .with_planning_rejection_policy(PlanningRejectionPolicy::SkipRejectedTargets),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect("stream should skip incompatible attempt and open fallback");

    while stream.next().await.is_some() {}
    let completion = stream.finish().await.expect("finish should succeed");
    assert_eq!(
        completion.meta.selected_provider_kind,
        ProviderKind::OpenRouter
    );

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec![
            "request_start",
            "attempt_skipped",
            "attempt_start",
            "attempt_success",
            "request_end",
        ]
    );

    let skipped = as_attempt_skipped(&events[1]);
    assert_eq!(
        skipped.provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(skipped.provider_kind, ProviderKind::OpenAi);
    assert_eq!(skipped.model, "gpt-5-mini");
    assert_eq!(skipped.target_index, 0);
    assert_eq!(skipped.attempt_index, 0);
    assert!(matches!(
        skipped.reason,
        SkipReason::StaticIncompatibility { .. }
    ));
}

#[tokio::test]
async fn routed_streaming_planning_failure_emits_request_end_after_attempt_skipped() {
    let observer = Arc::new(RecordingObserver::new());
    let toolkit = AgentToolkit {
        clients: HashMap::from([(
            ProviderInstanceId::openai_default(),
            test_provider_client_with_streaming_support(
                ProviderKind::OpenAi,
                Some("gpt-5-mini"),
                false,
            ),
        )]),
        observer: Some(observer.clone()),
    };

    let error = toolkit
        .streaming()
        .create_with_options(
            MessageCreateInput::user("hello"),
            Route::to(Target::new(ProviderInstanceId::openai_default()))
                .with_planning_rejection_policy(PlanningRejectionPolicy::FailFast),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect_err("planning rejection must stop routing");

    assert_eq!(error.kind, RuntimeErrorKind::TargetResolution);

    let events = observer.snapshot();
    assert_eq!(
        event_names(&events),
        vec!["request_start", "attempt_skipped", "request_end"]
    );

    let skipped = as_attempt_skipped(&events[1]);
    assert_eq!(
        skipped.provider_instance,
        ProviderInstanceId::openai_default()
    );
    assert_eq!(skipped.provider_kind, ProviderKind::OpenAi);
    assert_eq!(skipped.model, "gpt-5-mini");
    assert!(matches!(
        skipped.reason,
        SkipReason::StaticIncompatibility { .. }
    ));
}
