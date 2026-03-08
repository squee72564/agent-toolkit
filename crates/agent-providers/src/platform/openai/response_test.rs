use agent_core::{ContentPart, ResponseFormat};
use serde_json::json;

use crate::platform::openai::response::decode_response_json;

#[test]
fn openai_response_decoder_uses_existing_openai_decode_path() {
    let response = decode_response_json(
        json!({
            "status": "completed",
            "model": "gpt-5-mini",
            "output": [{
                "type": "message",
                "content": [{ "type": "output_text", "text": "hello" }]
            }],
            "usage": {
                "input_tokens": 1,
                "output_tokens": 2,
                "total_tokens": 3
            }
        }),
        &ResponseFormat::Text,
    )
    .expect("decode should succeed");

    assert_eq!(response.model, "gpt-5-mini");
    assert_eq!(response.output.content, vec![ContentPart::text("hello")]);
}
