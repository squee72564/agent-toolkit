use std::env;
use std::sync::Arc;

use agent_toolkit::core::{CanonicalStreamEvent, StreamOutputItemEnd, StreamOutputItemStart};
use agent_toolkit::tools::{ToolBuilder, ToolRegistry, ToolRuntime};
use agent_toolkit::{
    AttemptFailureEvent, AttemptStartEvent, AttemptSuccessEvent, ContentPart, Conversation,
    MessageCreateInput, RequestEndEvent, RequestStartEvent, RuntimeObserver, ToolChoice, openai,
};
use futures_util::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct PrintObserver;

impl RuntimeObserver for PrintObserver {
    fn on_request_start(&self, event: &RequestStartEvent) {
        println!(
            "[request_start] provider={:?} model={:?}",
            event.provider, event.model
        );
    }

    fn on_attempt_start(&self, event: &AttemptStartEvent) {
        println!(
            "[attempt_start] provider={:?} model={:?}",
            event.provider, event.model
        );
    }

    fn on_attempt_success(&self, event: &AttemptSuccessEvent) {
        println!(
            "[attempt_success] provider={:?} status={:?}",
            event.provider, event.status_code
        );
    }

    fn on_attempt_failure(&self, event: &AttemptFailureEvent) {
        println!(
            "[attempt_failure] provider={:?} kind={:?}",
            event.provider, event.error_kind
        );
    }

    fn on_request_end(&self, event: &RequestEndEvent) {
        println!(
            "[request_end] status={:?} error_kind={:?}",
            event.status_code, event.error_kind
        );
    }
}

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

    let mut registry = ToolRegistry::new();
    registry.register_validated(weather_tool)?;
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

    let observer: Arc<dyn RuntimeObserver> = Arc::new(PrintObserver);
    let client = openai()
        .api_key(env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .observer(observer)
        .build()?;

    let registry = build_registry()?;
    let prompt = "Call get_weather for San Francisco, CA. After the tool result arrives, reply in one short sentence.";

    let mut conversation = Conversation::new();
    conversation.push_user_text(prompt);

    let input = MessageCreateInput::user(prompt)
        .with_tools(registry.tool_definitions())
        .with_tool_choice(ToolChoice::Specific {
            name: "get_weather".to_string(),
        });

    let mut stream = client.streaming().create(input).await?;

    while let Some(envelope) = stream.next().await {
        let envelope = envelope?;
        for event in envelope.canonical {
            match event {
                CanonicalStreamEvent::ResponseStarted { model, .. } => {
                    println!("[response_started] model={model:?}");
                }
                CanonicalStreamEvent::OutputItemStarted { item, .. } => match item {
                    StreamOutputItemStart::Message { role, .. } => {
                        println!("[item_started] message role={role:?}");
                    }
                    StreamOutputItemStart::ToolCall { name, .. } => {
                        println!("[item_started] tool_call name={name}");
                    }
                },
                CanonicalStreamEvent::TextDelta { delta, .. } => {
                    print!("{delta}");
                }
                CanonicalStreamEvent::ToolCallArgumentsDelta {
                    tool_name, delta, ..
                } => {
                    println!(
                        "\n[tool_delta] tool={} delta={delta}",
                        tool_name.unwrap_or_else(|| "<unknown>".to_string())
                    );
                }
                CanonicalStreamEvent::OutputItemCompleted { item, .. } => match item {
                    StreamOutputItemEnd::Message { .. } => {
                        println!("\n[item_completed] message");
                    }
                    StreamOutputItemEnd::ToolCall {
                        name,
                        arguments_json_text,
                        ..
                    } => {
                        println!(
                            "[item_completed] tool_call name={name} args={arguments_json_text}"
                        );
                    }
                },
                CanonicalStreamEvent::Completed { finish_reason } => {
                    println!("[completed] finish_reason={finish_reason:?}");
                }
                CanonicalStreamEvent::UsageUpdated { usage } => {
                    println!("[usage] total_tokens={:?}", usage.total_tokens);
                }
                CanonicalStreamEvent::Failed { message } => {
                    println!("[failed] {message}");
                }
            }
        }
    }
    println!();

    let completion = stream.finish().await?;
    if let Some(next_input) =
        execute_tool_calls(&completion.response, &mut conversation, &registry).await?
    {
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
        println!(
            "assistant: {}",
            response_text(&completion.response.output.content)
        );
    }

    Ok(())
}
