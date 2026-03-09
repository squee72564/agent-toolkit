use agent_core::{ContentPart, ResponseFormat};
use serde_json::json;

use crate::platform::anthropic::response::decode_response_json;

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
