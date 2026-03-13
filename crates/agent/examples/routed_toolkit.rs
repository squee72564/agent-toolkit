use std::env;

use agent_toolkit::{
    AgentToolkit, ExecutionOptions, FallbackMode, FallbackPolicy, FallbackRule, MessageCreateInput,
    ProviderConfig, ProviderId, Route, Target,
};

fn response_text(parts: &[agent_toolkit::ContentPart]) -> String {
    let mut text = String::new();
    for part in parts {
        if let agent_toolkit::ContentPart::Text { text: delta } = part {
            text.push_str(delta);
        }
    }
    text
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new(env::var("OPENAI_API_KEY")?).with_default_model("gpt-5-mini"),
        )
        .with_openrouter(
            ProviderConfig::new(env::var("OPENROUTER_API_KEY")?)
                .with_default_model("openai/gpt-5-nano"),
        )
        .build()?;

    let fallback_policy = FallbackPolicy::new()
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_status(429))
        .with_rule(FallbackRule::retry_on_provider_code("rate_limit_exceeded"));
    let task =
        MessageCreateInput::user("Write one short sentence about Rust.").into_task_request()?;
    let route = Route::to(Target::new(ProviderId::OpenAi))
        .with_fallback(Target::new(ProviderId::OpenRouter))
        .with_fallback_policy(fallback_policy);

    let (response, meta) = toolkit
        .messages()
        .create_task_with_meta(task, route, ExecutionOptions::default())
        .await?;

    println!("selected_provider: {:?}", meta.selected_provider);
    println!("selected_model: {}", meta.selected_model);
    println!("assistant: {}", response_text(&response.output.content));

    Ok(())
}
