use agent_runtime::{MessageCreateInput, RuntimeErrorKind, openai};

#[tokio::test]
async fn public_messages_api_rejects_stream_requests_until_streaming_surface_exists() {
    let client = openai()
        .api_key("test-key")
        .base_url("http://127.0.0.1:1")
        .default_model("gpt-5-mini")
        .build()
        .expect("build direct client");

    let error = client
        .messages()
        .create(
            MessageCreateInput::user("hello")
                .with_model("gpt-5-mini")
                .with_stream(true),
        )
        .await
        .expect_err("current public response API should reject stream=true");

    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
    assert!(error.message.contains("stream=true is not supported"));
}
