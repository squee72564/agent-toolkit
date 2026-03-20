use std::collections::BTreeMap;
use std::env;

use agent_toolkit::prelude::{Message, MessageCreateInput, MessageRole, ToolChoice, openai};
use agent_toolkit::request::ResponseFormat;
use agent_toolkit::runtime::ExecutionOptions;
use agent_toolkit::{ContentPart, ToolDefinition};
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

    let task = MessageCreateInput::new(vec![Message::new(
        MessageRole::User,
        vec![ContentPart::text(
            "Write one sentence about explicit request construction.",
        )],
    )])
    .with_tools(vec![ToolDefinition {
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
    }])
    .with_tool_choice(ToolChoice::Auto)
    .with_response_format(ResponseFormat::Text)
    .with_max_output_tokens(128)
    .with_metadata(metadata)
    .into_task_request()?;

    let (response, meta) = client
        .messages()
        .create_task_with_meta(task, ExecutionOptions::default())
        .await?;

    println!("selected_provider: {:?}", meta.selected_provider_kind);
    println!("selected_model: {}", meta.selected_model);
    println!("assistant: {}", response_text(&response.output.content));

    Ok(())
}
