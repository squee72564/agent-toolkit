use agent_runtime::{ExecutionOptions, MessageCreateInput, ResponseMode, RuntimeErrorKind, openai};

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
        .create_task(
            MessageCreateInput::user("hello")
                .into_task_request()
                .expect("task request should build"),
            ExecutionOptions {
                response_mode: ResponseMode::Streaming,
                ..ExecutionOptions::default()
            },
        )
        .await
        .expect_err("messages() should reject streaming execution options");

    assert_eq!(error.kind, RuntimeErrorKind::Configuration);
    assert!(error.message.contains("ResponseMode::NonStreaming"));
}
