use agent_core::{Message, ResponseFormat, ResponseMode, TaskRequest, ToolChoice};

use crate::family_codec::codec_for;
use crate::request_plan::TransportResponseFraming;
use reqwest::Method;

const MODEL_ID: &str = "gpt-5-mini";

fn base_task() -> TaskRequest {
    TaskRequest {
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
fn openai_request_plan_uses_json_defaults_for_non_streaming_requests() {
    let plan = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&base_task(), MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    assert_eq!(plan.method, Method::POST);
    assert_eq!(plan.response_framing, TransportResponseFraming::Json);
    assert_eq!(plan.body["model"], "gpt-5-mini");
    assert!(plan.body.get("stream").is_none());
    assert!(plan.request_options.allow_error_status);
}

#[test]
fn openai_request_plan_enables_sse_for_streaming_requests() {
    let plan = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible)
        .encode_task(&base_task(), MODEL_ID, ResponseMode::Streaming, None)
        .expect("planning should succeed");

    assert_eq!(plan.method, Method::POST);
    assert_eq!(plan.response_framing, TransportResponseFraming::Sse);
    assert_eq!(plan.body["stream"], true);
    assert_eq!(
        plan.request_options.expected_content_type.as_deref(),
        Some("text/event-stream")
    );
}
