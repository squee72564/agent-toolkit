use std::env;

use agent_toolkit::ContentPart;
use agent_toolkit::prelude::{Conversation, MessageCreateInput, ToolChoice, openai};
use agent_toolkit::tools::{ToolBuilder, ToolOutput, ToolRegistry, ToolRuntime};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize, JsonSchema)]
struct WeatherArgs {
    city: String,
}

#[derive(Debug, Serialize)]
struct WeatherOut {
    city: String,
    temp_f: i32,
    conditions: String,
}

fn response_text(parts: &[ContentPart]) -> String {
    let mut text = String::new();
    for part in parts {
        if let ContentPart::Text { text: delta } = part {
            text.push_str(delta);
        }
    }
    text
}

fn build_registry() -> Result<ToolRegistry, Box<dyn std::error::Error>> {
    let weather_tool = ToolBuilder::new()
        .name("get_weather")
        .description("Get current weather by city")
        .typed_handler(|args: WeatherArgs| async move {
            Ok::<WeatherOut, agent_toolkit::tools::ToolError>(WeatherOut {
                city: args.city,
                temp_f: 67,
                conditions: "sunny".to_string(),
            })
        })
        .build()?;

    let raw_echo = ToolBuilder::new()
        .name("raw_echo")
        .description("Echo a JSON payload")
        .schema(json!({
            "type": "object",
            "properties": {
                "value": { "type": "string" }
            },
            "required": ["value"],
            "additionalProperties": false
        }))
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build()?;

    let mut registry = ToolRegistry::new();
    registry.register_validated(weather_tool)?;
    registry.register_validated(raw_echo)?;
    Ok(registry)
}

async fn execute_tool_calls(
    response: &agent_toolkit::Response,
    conversation: &mut Conversation,
    registry: &ToolRegistry,
) -> Result<Option<MessageCreateInput>, Box<dyn std::error::Error>> {
    let runtime = ToolRuntime::new(registry);
    let mut saw_tool_call = false;

    for part in &response.output.content {
        if let ContentPart::ToolCall { tool_call } = part {
            saw_tool_call = true;
            conversation.push_assistant_tool_call(
                tool_call.id.clone(),
                tool_call.name.clone(),
                tool_call.arguments_json.clone(),
            );

            let output = runtime
                .execute(&tool_call.name, tool_call.arguments_json.clone())
                .await?;
            conversation.push_tool_result_json(tool_call.id.clone(), output.content);
        }
    }

    if saw_tool_call {
        Ok(Some(conversation.to_input()))
    } else {
        Ok(None)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let client = openai()
        .api_key(env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .build()?;

    let registry = build_registry()?;
    let prompt =
        "Call get_weather for San Francisco, CA, then summarize the result in one sentence.";

    let mut conversation = Conversation::new();
    conversation.push_user_text(prompt);

    let first = client
        .messages()
        .create(
            MessageCreateInput::user(prompt)
                .with_tools(registry.tool_definitions())
                .with_tool_choice(ToolChoice::Specific {
                    name: "get_weather".to_string(),
                }),
        )
        .await?;

    if let Some(next_input) = execute_tool_calls(&first, &mut conversation, &registry).await? {
        let follow_up = client
            .messages()
            .create(
                next_input
                    .with_tools(registry.tool_definitions())
                    .with_tool_choice(ToolChoice::Auto),
            )
            .await?;
        println!("assistant: {}", response_text(&follow_up.output.content));
    } else {
        println!("assistant: {}", response_text(&first.output.content));
    }

    Ok(())
}
