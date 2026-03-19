use std::cell::RefCell;
use std::rc::Rc;

use reqwest::Method;
use serde_json::json;

use agent_core::types::{ProviderKind, ResponseFormat, ResponseMode};

use crate::interfaces::adapter_for;
use crate::adapter::tests::shared::{
    base_task, compose_openai_compatible_request,
    create_stream_projector_with_composition_test_hook, execution_plan,
};
use crate::error::AdapterErrorKind;
use crate::openai_family::OpenAiDecodeEnvelope;
use crate::openai_family::decode::decode_openai_response;
use crate::request_plan::TransportResponseFraming;
use crate::interfaces::ProviderStreamProjector;

const OPENAI_MODEL: &str = "openai/gpt-5-mini";

#[test]
fn openrouter_adapter_plan_request_matches_family_refinement_translation() {
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

    let translated = compose_openai_compatible_request(
        ProviderKind::OpenRouter,
        &task,
        OPENAI_MODEL,
        ResponseMode::NonStreaming,
        None,
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
fn openrouter_streaming_plan_preserves_family_default_request_contract() {
    let task = base_task();
    let execution = execution_plan(
        ProviderKind::OpenRouter,
        &task,
        OPENAI_MODEL,
        ResponseMode::Streaming,
        None,
    );
    let adapter_plan = adapter_for(ProviderKind::OpenRouter)
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
fn adapters_decode_responses_with_existing_translators() {
    let format = ResponseFormat::Text;
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
fn adapters_expose_layered_error_decode_contract() {
    let openrouter_error = adapter_for(ProviderKind::OpenRouter)
        .decode_error(&json!({
            "error": {
                "message": "openai/this-model-does-not-exist is not a valid model ID",
                "code": "400",
            }
        }))
        .expect("error info should decode");
    assert_eq!(openrouter_error.provider_code.as_deref(), Some("400"));
    assert_eq!(openrouter_error.kind, Some(AdapterErrorKind::Upstream));
    assert!(openrouter_error.message.as_deref().is_some_and(|message| {
        message.contains("openai/this-model-does-not-exist is not a valid model ID")
    }));
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
fn create_stream_projector_uses_refinement_override_before_family_fallback() {
    let call_order = Rc::new(RefCell::new(Vec::new()));
    let refinement_order = Rc::clone(&call_order);
    let family_order = Rc::clone(&call_order);

    let mut projector = create_stream_projector_with_composition_test_hook(
        move || {
            refinement_order.borrow_mut().push("refinement");
            Some(Box::new(MarkerProjector {
                marker: "refinement",
            }))
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

    assert_eq!(&*call_order.borrow(), &["refinement"]);
    assert_eq!(
        events,
        vec![agent_core::CanonicalStreamEvent::ResponseStarted {
            model: Some("refinement".to_string()),
            response_id: None,
        }]
    );
}

#[test]
fn openrouter_adapter_stream_projector_uses_refinement_override() {
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
