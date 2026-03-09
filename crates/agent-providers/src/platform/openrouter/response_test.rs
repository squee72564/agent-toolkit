use agent_core::ResponseFormat;
use serde_json::json;

use crate::platform::openrouter::response::decode_response_json;

#[test]
fn openrouter_response_decoder_rejects_chat_completions_payloads() {
    let error = decode_response_json(
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
    .expect_err("decode should fail");

    assert!(!error.message.is_empty());
}
