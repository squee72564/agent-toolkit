use std::collections::BTreeMap;
use std::io;

use agent_core::types::{
    AssistantOutput, ContentPart, FinishReason, Message, Request, Response, ResponseFormat,
    ToolChoice, Usage,
};
use agent_providers::translator_contract::ProtocolTranslator;

#[derive(Debug, Clone, Copy)]
struct EchoTranslator;

impl ProtocolTranslator for EchoTranslator {
    type RequestPayload = String;
    type ResponsePayload = String;
    type Error = io::Error;

    fn encode_request(&self, req: Request) -> Result<Self::RequestPayload, Self::Error> {
        if req.model_id.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty model_id",
            ));
        }
        Ok(req.model_id)
    }

    fn decode_request(&self, payload: Self::ResponsePayload) -> Result<Response, Self::Error> {
        if payload.trim().is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "empty payload"));
        }

        Ok(Response {
            output: AssistantOutput {
                content: vec![ContentPart::text(payload)],
                structured_output: None,
            },
            usage: Usage::default(),
            model: "mock-model".to_string(),
            raw_provider_response: None,
            finish_reason: FinishReason::Stop,
            warnings: Vec::new(),
        })
    }
}

fn request_with_model(model_id: &str) -> Request {
    Request {
        model_id: model_id.to_string(),
        messages: vec![Message::user_text("hello")],
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

#[test]
fn encode_request_returns_expected_payload() {
    let translator = EchoTranslator;
    let payload = translator
        .encode_request(request_with_model("gpt-4.1-mini"))
        .expect("encode should succeed");

    assert_eq!(payload, "gpt-4.1-mini");
}

#[test]
fn decode_request_returns_expected_response() {
    let translator = EchoTranslator;
    let response = translator
        .decode_request("hello from provider".to_string())
        .expect("decode should succeed");

    assert_eq!(
        response.output.content,
        vec![ContentPart::text("hello from provider")]
    );
    assert_eq!(response.model, "mock-model");
    assert_eq!(response.finish_reason, FinishReason::Stop);
}

#[test]
fn encode_request_error_is_propagated() {
    let translator = EchoTranslator;
    let error = translator
        .encode_request(request_with_model("   "))
        .expect_err("empty model id should fail");

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("empty model_id"));
}

#[test]
fn decode_request_error_is_propagated() {
    let translator = EchoTranslator;
    let error = translator
        .decode_request("   ".to_string())
        .expect_err("empty payload should fail");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("empty payload"));
}
