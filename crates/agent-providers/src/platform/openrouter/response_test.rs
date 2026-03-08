use agent_core::{ContentPart, ResponseFormat};
use serde_json::json;

use crate::platform::openrouter::response::decode_response_json;

#[test]
fn openrouter_response_decoder_handles_chat_completions_fallback() {
    let response = decode_response_json(
        json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "openai/gpt-5-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": "hello"
                }
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 6,
                "total_tokens": 11
            }
        }),
        &ResponseFormat::Text,
    )
    .expect("decode should succeed");

    assert_eq!(response.model, "openai/gpt-5-mini");
    assert_eq!(response.output.content, vec![ContentPart::text("hello")]);
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "openrouter.decode.fallback_chat_completions")
    );
}
