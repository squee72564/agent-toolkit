use agent_core::{Message, Request, ResponseFormat, ToolChoice};

use crate::platform::openrouter::request::{OpenRouterOverrides, plan_request};
use crate::request_plan::{ProviderResponseKind, ProviderTransportKind};

fn base_request(stream: bool) -> Request {
    Request {
        model_id: "openai/gpt-5-mini".to_string(),
        stream,
        messages: vec![Message::user_text("hello")],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: Default::default(),
    }
}

#[test]
fn openrouter_request_plan_uses_json_defaults_for_non_streaming_requests() {
    let plan = plan_request(base_request(false), &OpenRouterOverrides::default())
        .expect("planning should succeed");

    assert_eq!(plan.transport_kind, ProviderTransportKind::HttpJson);
    assert_eq!(plan.response_kind, ProviderResponseKind::JsonBody);
    assert!(plan.body.get("stream").is_none());
    assert!(plan.request_options.allow_error_status);
}

#[test]
fn openrouter_request_plan_enables_sse_for_streaming_requests() {
    let plan = plan_request(base_request(true), &OpenRouterOverrides::default())
        .expect("planning should succeed");

    assert_eq!(plan.transport_kind, ProviderTransportKind::HttpSse);
    assert_eq!(plan.response_kind, ProviderResponseKind::RawProviderStream);
    assert_eq!(plan.body["stream"], true);
}
