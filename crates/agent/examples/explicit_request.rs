use std::collections::BTreeMap;
use std::env;

use agent_toolkit::{
    ContentPart, Message, MessageRole, Request, ResponseFormat, ToolChoice, ToolDefinition, openai,
};
use serde_json::json;

fn response_text(parts: &[ContentPart]) -> String {
    let mut text = String::new();
    for part in parts {
        if let ContentPart::Text { text: delta } = part {
            text.push_str(delta);
        }
    }
    text
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let client = openai()
        .api_key(env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .build()?;

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "trace_id".to_string(),
        "example-explicit-request".to_string(),
    );

    let request = Request {
        model_id: "gpt-5-mini".to_string(),
        stream: false,
        messages: vec![Message::new(
            MessageRole::User,
            vec![ContentPart::text(
                "Write one sentence about explicit request construction.",
            )],
        )],
        tools: vec![ToolDefinition {
            name: "raw_echo".to_string(),
            description: Some("Echo the provided value".to_string()),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                },
                "required": ["value"],
                "additionalProperties": false
            }),
        }],
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: Some(128),
        stop: Vec::new(),
        metadata,
    };

    let (response, meta) = client.messages().create_request_with_meta(request).await?;

    println!("selected_provider: {:?}", meta.selected_provider);
    println!("selected_model: {}", meta.selected_model);
    println!("assistant: {}", response_text(&response.output.content));

    Ok(())
}
