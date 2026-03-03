use std::collections::BTreeMap;

use serde_json::json;

use crate::core::types::{ContentPart, Message, MessageRole, Request, ResponseFormat, ToolChoice};

use super::decode::decode_anthropic_response;
use super::encode::encode_anthropic_request;
use super::{AnthropicDecodeEnvelope, AnthropicSpecError};

fn base_request(messages: Vec<Message>) -> Request {
    Request {
        model_id: "claude-sonnet-4.5".to_string(),
        messages,
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
fn encode_is_explicitly_not_implemented_yet() {
    let request = base_request(vec![Message {
        role: MessageRole::User,
        content: vec![ContentPart::Text {
            text: "hello".to_string(),
        }],
    }]);

    let error = encode_anthropic_request(&request).expect_err("encoding should be stubbed");
    match error {
        AnthropicSpecError::UnsupportedFeature { message } => {
            assert_eq!(message, "Anthropic encode not implemented");
        }
        other => panic!("expected unsupported feature error, got {other:?}"),
    }
}

#[test]
fn decode_is_explicitly_not_implemented_yet() {
    let envelope = AnthropicDecodeEnvelope {
        body: json!({}),
        requested_response_format: ResponseFormat::Text,
    };

    let error = decode_anthropic_response(&envelope).expect_err("decoding should be stubbed");
    match error {
        AnthropicSpecError::UnsupportedFeature { message } => {
            assert_eq!(message, "Anthropic decode not implemented");
        }
        other => panic!("expected unsupported feature error, got {other:?}"),
    }
}
