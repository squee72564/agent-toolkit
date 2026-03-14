use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use reqwest::Method;
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::json;

use agent_core::types::{
    AssistantOutput, AuthCredentials, AuthStyle, ContentPart, ExecutionPlan, FinishReason, Message,
    MessageRole, NativeOptions, PlatformConfig, ProtocolKind, ProviderCapabilities,
    ProviderInstanceId, ProviderKind, ResolvedAuthContext, ResolvedProviderAttempt,
    ResolvedTransportOptions, Response, ResponseFormat, ResponseMode, TaskRequest, ToolChoice,
    TransportTimeoutOverrides, Usage,
};

use crate::anthropic_family::AnthropicDecodeEnvelope;
use crate::anthropic_family::decode::decode_anthropic_response;
use crate::error::{AdapterErrorKind, ProviderErrorInfo};
use crate::openai_family::OpenAiDecodeEnvelope;
use crate::openai_family::decode::decode_openai_response;
use crate::platform::anthropic::request as anthropic_request;
use crate::platform::openai::request as openai_request;
use crate::platform::openrouter::request as openrouter_request;
use crate::request_plan::TransportResponseFraming;
use crate::streaming::ProviderStreamProjector;

use super::*;

const OPENAI_MODEL: &str = "openai/gpt-5-mini";
const ANTHROPIC_MODEL: &str = "claude-sonnet-4-6";

fn base_task() -> TaskRequest {
    TaskRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "hello".to_string(),
            }],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

fn execution_plan(
    provider: ProviderKind,
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
    native_options: Option<NativeOptions>,
) -> ExecutionPlan {
    let adapter = adapter_for(provider);
    let instance_id = match provider {
        ProviderKind::OpenAi => ProviderInstanceId::openai_default(),
        ProviderKind::Anthropic => ProviderInstanceId::anthropic_default(),
        ProviderKind::OpenRouter => ProviderInstanceId::openrouter_default(),
        ProviderKind::GenericOpenAiCompatible => {
            ProviderInstanceId::generic_openai_compatible_default()
        }
    };
    ExecutionPlan {
        response_mode,
        task: task.clone(),
        provider_attempt: ResolvedProviderAttempt {
            instance_id,
            provider_kind: provider,
            family: adapter.descriptor().family,
            model: model.to_string(),
            capabilities: *adapter.capabilities(),
            native_options,
        },
        platform: PlatformConfig {
            protocol: adapter.descriptor().protocol.clone(),
            base_url: adapter.descriptor().default_base_url.to_string(),
            auth_style: adapter.descriptor().default_auth_style.clone(),
            request_id_header: adapter.descriptor().default_request_id_header.clone(),
            default_headers: adapter.descriptor().default_headers.clone(),
        },
        auth: ResolvedAuthContext {
            credentials: Some(AuthCredentials::Token("test-key".to_string())),
        },
        transport: ResolvedTransportOptions {
            request_id_header_override: None,
            route_extra_headers: BTreeMap::new(),
            attempt_extra_headers: BTreeMap::new(),
            timeouts: TransportTimeoutOverrides::default(),
            retry_policy: agent_core::RetryPolicy::default(),
        },
        capabilities: ProviderCapabilities {
            supports_streaming: true,
            supports_family_native_options: true,
            supports_provider_native_options: true,
        },
    }
}

#[test]
fn adapter_lookup_returns_expected_kinds() {
    assert_eq!(
        adapter_for(ProviderKind::OpenAi).kind(),
        ProviderKind::OpenAi
    );
    assert_eq!(
        adapter_for(ProviderKind::Anthropic).kind(),
        ProviderKind::Anthropic
    );
    assert_eq!(
        adapter_for(ProviderKind::OpenRouter).kind(),
        ProviderKind::OpenRouter
    );
    assert_eq!(
        adapter_for(ProviderKind::GenericOpenAiCompatible).kind(),
        ProviderKind::GenericOpenAiCompatible
    );
}

#[test]
fn all_builtin_adapters_contains_all_known_providers() {
    let ids: Vec<ProviderKind> = all_builtin_adapters()
        .iter()
        .map(|adapter| adapter.kind())
        .collect();
    assert_eq!(ids.len(), 4);
    assert!(ids.contains(&ProviderKind::OpenAi));
    assert!(ids.contains(&ProviderKind::Anthropic));
    assert!(ids.contains(&ProviderKind::OpenRouter));
    assert!(ids.contains(&ProviderKind::GenericOpenAiCompatible));
}

#[test]
fn descriptors_expose_expected_static_metadata() {
    let openai = adapter_for(ProviderKind::OpenAi).descriptor();
    assert_eq!(openai.protocol, ProtocolKind::OpenAI);
    assert_eq!(openai.default_auth_style, AuthStyle::Bearer);
    assert_eq!(openai.default_base_url, "https://api.openai.com");
    assert_eq!(openai.endpoint_path, "/v1/responses");

    let anthropic = adapter_for(ProviderKind::Anthropic).descriptor();
    assert_eq!(anthropic.protocol, ProtocolKind::Anthropic);
    assert_eq!(
        anthropic.default_auth_style,
        AuthStyle::ApiKeyHeader(HeaderName::from_static("x-api-key"))
    );
    assert_eq!(
        anthropic
            .default_headers
            .get(HeaderName::from_static("anthropic-version")),
        Some(&HeaderValue::from_static("2023-06-01"))
    );
}

#[test]
fn openai_adapter_plan_request_matches_family_overlay_translation() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::OpenAi,
        &task,
        OPENAI_MODEL,
        ResponseMode::NonStreaming,
        None,
    );

    let translated = openai_request::plan_request(
        &task,
        OPENAI_MODEL,
        ResponseMode::NonStreaming,
        ProviderKind::OpenAi,
        None,
    )
    .expect("request planning should succeed");
    let adapter_plan = adapter_for(ProviderKind::OpenAi)
        .plan_request(&execution)
        .expect("adapter planning should succeed");

    assert_eq!(adapter_plan.body, translated.body);
    assert_eq!(adapter_plan.warnings, translated.warnings);
    assert_eq!(adapter_plan.method, Method::POST);
    assert_eq!(
        adapter_plan.response_framing,
        TransportResponseFraming::Json
    );
}

#[test]
fn anthropic_adapter_plan_request_matches_family_overlay_translation() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::Anthropic,
        &task,
        ANTHROPIC_MODEL,
        ResponseMode::NonStreaming,
        None,
    );

    let translated =
        anthropic_request::plan_request(&task, ANTHROPIC_MODEL, ResponseMode::NonStreaming, None)
            .expect("planning should succeed");
    let adapter_plan = adapter_for(ProviderKind::Anthropic)
        .plan_request(&execution)
        .expect("adapter planning should succeed");

    assert_eq!(adapter_plan.body, translated.body);
    assert_eq!(adapter_plan.warnings, translated.warnings);
    assert_eq!(adapter_plan.method, Method::POST);
    assert_eq!(
        adapter_plan.response_framing,
        TransportResponseFraming::Json
    );
}

#[test]
fn openrouter_adapter_plan_request_matches_family_overlay_translation() {
    let mut task = base_task();
    task.top_p = Some(0.5);
    task.stop = vec!["done".to_string()];
    let execution = execution_plan(
        ProviderKind::OpenRouter,
        &task,
        OPENAI_MODEL,
        ResponseMode::NonStreaming,
        None,
    );

    let translated = openrouter_request::plan_request(
        &task,
        OPENAI_MODEL,
        ResponseMode::NonStreaming,
        &openrouter_request::OpenRouterOverrides::default(),
    )
    .expect("planning should succeed");
    let adapter_plan = adapter_for(ProviderKind::OpenRouter)
        .plan_request(&execution)
        .expect("adapter planning should succeed");

    assert_eq!(adapter_plan.body, translated.body);
    assert_eq!(adapter_plan.warnings, translated.warnings);
    assert_eq!(adapter_plan.method, Method::POST);
    assert_eq!(
        adapter_plan.response_framing,
        TransportResponseFraming::Json
    );
    assert!(adapter_plan.endpoint_path_override.is_none());
    assert!(adapter_plan.provider_headers.is_empty());
    assert!(adapter_plan.request_options.allow_error_status);
}

#[test]
fn openai_streaming_plan_preserves_family_default_request_contract() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::OpenAi,
        &task,
        OPENAI_MODEL,
        ResponseMode::Streaming,
        None,
    );
    let adapter_plan = adapter_for(ProviderKind::OpenAi)
        .plan_request(&execution)
        .expect("adapter planning should succeed");

    assert_eq!(adapter_plan.method, Method::POST);
    assert_eq!(adapter_plan.response_framing, TransportResponseFraming::Sse);
    assert!(adapter_plan.endpoint_path_override.is_none());
    assert!(adapter_plan.provider_headers.is_empty());
    let expected = agent_transport::HttpRequestOptions::sse_defaults();
    assert_eq!(
        adapter_plan.request_options.allow_error_status,
        expected.allow_error_status
    );
}

#[test]
fn anthropic_non_streaming_plan_preserves_family_default_request_contract() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::Anthropic,
        &task,
        ANTHROPIC_MODEL,
        ResponseMode::NonStreaming,
        None,
    );
    let adapter_plan = adapter_for(ProviderKind::Anthropic)
        .plan_request(&execution)
        .expect("adapter planning should succeed");

    assert_eq!(adapter_plan.method, Method::POST);
    assert_eq!(
        adapter_plan.response_framing,
        TransportResponseFraming::Json
    );
    assert!(adapter_plan.endpoint_path_override.is_none());
    assert!(adapter_plan.provider_headers.is_empty());
    assert!(adapter_plan.request_options.allow_error_status);
}

#[test]
fn adapters_decode_responses_with_existing_translators() {
    let openai_body = json!({
        "status": "completed",
        "model": "gpt-5-mini",
        "output": [{ "type": "message", "content": [{ "type": "output_text", "text": "hello" }] }],
        "usage": { "input_tokens": 1, "output_tokens": 2, "total_tokens": 3 }
    });
    let format = ResponseFormat::Text;
    assert_eq!(
        adapter_for(ProviderKind::OpenAi)
            .decode_response_json(openai_body.clone(), &format)
            .expect("decode should succeed"),
        decode_openai_response(&OpenAiDecodeEnvelope {
            body: openai_body,
            requested_response_format: format.clone(),
        })
        .expect("decode should succeed")
    );

    let anthropic_body = json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "model": "claude-sonnet-4-6",
        "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": "hello" }],
        "usage": { "input_tokens": 1, "output_tokens": 2 }
    });
    assert_eq!(
        adapter_for(ProviderKind::Anthropic)
            .decode_response_json(anthropic_body.clone(), &format)
            .expect("decode should succeed"),
        decode_anthropic_response(&AnthropicDecodeEnvelope {
            body: anthropic_body,
            requested_response_format: format.clone(),
        })
        .expect("decode should succeed")
    );

    let openrouter_body = json!({
        "status": "completed",
        "model": "openrouter/model",
        "output": [{ "type": "message", "content": [{ "type": "output_text", "text": "hello" }] }]
    });
    assert_eq!(
        adapter_for(ProviderKind::OpenRouter)
            .decode_response_json(openrouter_body.clone(), &format)
            .expect("decode should succeed"),
        decode_openai_response(&OpenAiDecodeEnvelope {
            body: openrouter_body,
            requested_response_format: format.clone(),
        })
        .expect("decode should succeed")
    );
}

#[test]
fn decode_response_uses_overlay_override_before_family_fallback() {
    let format = ResponseFormat::Text;
    let call_order = Rc::new(RefCell::new(Vec::new()));
    let overlay_order = Rc::clone(&call_order);
    let family_order = Rc::clone(&call_order);

    let response = super::decode_response_with_composition_test_hook(
        json!({ "ignored": true }),
        &format,
        move |_body, _requested_format| {
            overlay_order.borrow_mut().push("overlay");
            Some(Ok(Response {
                output: AssistantOutput {
                    content: vec![ContentPart::Text {
                        text: "overlay".to_string(),
                    }],
                    structured_output: None,
                },
                usage: Usage::default(),
                model: "overlay-model".to_string(),
                raw_provider_response: None,
                finish_reason: FinishReason::Stop,
                warnings: Vec::new(),
            }))
        },
        move |_body, _requested_format| {
            family_order.borrow_mut().push("family");
            Ok(Response {
                output: AssistantOutput {
                    content: vec![ContentPart::Text {
                        text: "family".to_string(),
                    }],
                    structured_output: None,
                },
                usage: Usage::default(),
                model: "family-model".to_string(),
                raw_provider_response: None,
                finish_reason: FinishReason::Stop,
                warnings: Vec::new(),
            })
        },
        |error| error,
    )
    .expect("decode should succeed");

    assert_eq!(
        response.output.content,
        vec![ContentPart::Text {
            text: "overlay".to_string(),
        }]
    );
    assert_eq!(&*call_order.borrow(), &["overlay"]);
}

#[test]
fn decode_response_refines_family_error_after_family_decode_runs() {
    let format = ResponseFormat::Text;
    let call_order = Rc::new(RefCell::new(Vec::new()));
    let family_order = Rc::clone(&call_order);
    let refine_order = Rc::clone(&call_order);

    let error = super::decode_response_with_composition_test_hook(
        json!({ "error": true }),
        &format,
        |_body, _requested_format| None,
        move |_body, _requested_format| {
            family_order.borrow_mut().push("family");
            Err(crate::error::AdapterError::new(
                crate::error::AdapterErrorKind::Upstream,
                ProviderKind::OpenAi,
                crate::error::AdapterOperation::DecodeResponse,
                "family failure",
            ))
        },
        move |mut error| {
            refine_order.borrow_mut().push("overlay-refine");
            error.message = format!("refined: {}", error.message);
            error
        },
    )
    .expect_err("decode should fail");

    assert_eq!(error.message, "refined: family failure");
    assert_eq!(&*call_order.borrow(), &["family", "overlay-refine"]);
}

#[test]
fn decode_error_runs_family_before_overlay_refinement() {
    let call_order = Rc::new(RefCell::new(Vec::new()));
    let family_order = Rc::clone(&call_order);
    let overlay_order = Rc::clone(&call_order);

    let info = super::decode_error_with_composition_test_hook(
        move || {
            family_order.borrow_mut().push("family");
            Some(ProviderErrorInfo {
                provider_code: Some("family_code".to_string()),
                message: Some("family message".to_string()),
                kind: Some(AdapterErrorKind::Upstream),
            })
        },
        move || {
            overlay_order.borrow_mut().push("overlay");
            Some(ProviderErrorInfo {
                provider_code: Some("overlay_code".to_string()),
                message: None,
                kind: None,
            })
        },
    )
    .expect("error info should be present");

    assert_eq!(&*call_order.borrow(), &["family", "overlay"]);
    assert_eq!(info.provider_code.as_deref(), Some("overlay_code"));
    assert_eq!(info.message.as_deref(), Some("family message"));
    assert_eq!(info.kind, Some(AdapterErrorKind::Upstream));
}

#[test]
fn decode_error_overlay_fields_win_on_collision() {
    let info = super::decode_error_with_composition_test_hook(
        || {
            Some(ProviderErrorInfo {
                provider_code: Some("family_code".to_string()),
                message: Some("family message".to_string()),
                kind: Some(AdapterErrorKind::Decode),
            })
        },
        || {
            Some(ProviderErrorInfo {
                provider_code: Some("overlay_code".to_string()),
                message: Some("overlay message".to_string()),
                kind: Some(AdapterErrorKind::Upstream),
            })
        },
    )
    .expect("error info should be present");

    assert_eq!(info.provider_code.as_deref(), Some("overlay_code"));
    assert_eq!(info.message.as_deref(), Some("overlay message"));
    assert_eq!(info.kind, Some(AdapterErrorKind::Upstream));
}

#[test]
fn adapters_expose_layered_error_decode_contract() {
    let openai_error = adapter_for(ProviderKind::OpenAi)
        .decode_error(&json!({
            "error": {
                "message": "rate limited",
                "code": "rate_limit_exceeded",
                "type": "rate_limit"
            }
        }))
        .expect("error info should decode");
    assert_eq!(
        openai_error.provider_code.as_deref(),
        Some("rate_limit_exceeded")
    );
    assert_eq!(openai_error.kind, Some(AdapterErrorKind::Upstream));
    assert!(
        openai_error
            .message
            .as_deref()
            .is_some_and(|message| message.contains("rate limited"))
    );

    let anthropic_error = adapter_for(ProviderKind::Anthropic)
        .decode_error(&json!({
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "message": "bad input"
            },
            "request_id": "req_123"
        }))
        .expect("error info should decode");
    assert_eq!(
        anthropic_error.provider_code.as_deref(),
        Some("invalid_request_error")
    );
    assert_eq!(anthropic_error.kind, Some(AdapterErrorKind::Upstream));
    assert!(
        anthropic_error
            .message
            .as_deref()
            .is_some_and(|message| message.contains("bad input"))
    );
}

#[derive(Default)]
struct MarkerProjector {
    marker: &'static str,
}

impl ProviderStreamProjector for MarkerProjector {
    fn project(
        &mut self,
        _raw: agent_core::ProviderRawStreamEvent,
    ) -> Result<Vec<agent_core::CanonicalStreamEvent>, crate::error::AdapterError> {
        Ok(vec![agent_core::CanonicalStreamEvent::ResponseStarted {
            model: Some(self.marker.to_string()),
            response_id: None,
        }])
    }
}

#[test]
fn create_stream_projector_uses_overlay_override_before_family_fallback() {
    let call_order = Rc::new(RefCell::new(Vec::new()));
    let overlay_order = Rc::clone(&call_order);
    let family_order = Rc::clone(&call_order);

    let mut projector = super::create_stream_projector_with_composition_test_hook(
        move || {
            overlay_order.borrow_mut().push("overlay");
            Some(Box::new(MarkerProjector { marker: "overlay" }))
        },
        move || {
            family_order.borrow_mut().push("family");
            Box::new(MarkerProjector { marker: "family" })
        },
    );

    let events = projector
        .project(agent_core::ProviderRawStreamEvent::from_sse(
            ProviderKind::OpenRouter,
            1,
            None,
            None,
            None,
            "[DONE]",
        ))
        .expect("projection should succeed");

    assert_eq!(&*call_order.borrow(), &["overlay"]);
    assert_eq!(
        events,
        vec![agent_core::CanonicalStreamEvent::ResponseStarted {
            model: Some("overlay".to_string()),
            response_id: None,
        }]
    );
}

#[test]
fn openrouter_adapter_stream_projector_uses_overlay_override() {
    let mut projector = adapter_for(ProviderKind::OpenRouter).create_stream_projector();
    let events = projector
        .project(agent_core::ProviderRawStreamEvent::from_sse(
            ProviderKind::OpenRouter,
            1,
            None,
            None,
            None,
            "[DONE]",
        ))
        .expect("projection should succeed");

    assert_eq!(
        events,
        vec![agent_core::CanonicalStreamEvent::Completed {
            finish_reason: agent_core::FinishReason::Other,
        }]
    );
}
