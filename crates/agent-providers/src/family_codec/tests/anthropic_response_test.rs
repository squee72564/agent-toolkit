use agent_core::{ContentPart, ResponseFormat};
use serde_json::json;

use crate::interfaces::adapter_for;
use agent_core::ProviderKind;

#[test]
fn anthropic_response_decoder_uses_existing_decode_path() {
    let response = decode_response_json(
        json!({
            "role": "assistant",
            "model": "claude-sonnet-4-6",
            "stop_reason": "end_turn",
            "content": [{"type":"text","text":"hello"}],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        &ResponseFormat::Text,
    )
    .expect("decode should succeed");

    assert_eq!(response.model, "claude-sonnet-4-6");
    assert_eq!(response.output.content, vec![ContentPart::text("hello")]);
}

fn decode_response_json(
    body: serde_json::Value,
    requested_format: &ResponseFormat,
) -> Result<agent_core::Response, crate::error::AdapterError> {
    adapter_for(ProviderKind::Anthropic).decode_response_json(body, requested_format)
}
