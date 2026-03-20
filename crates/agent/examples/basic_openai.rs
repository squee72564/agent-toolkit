use std::env;

use agent_toolkit::prelude::{MessageCreateInput, openai};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let client = openai()
        .api_key(env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .build()?;

    let response = client
        .messages()
        .create(MessageCreateInput::user("Write one sentence about Rust."))
        .await?;

    println!("model: {}", response.model);
    println!("finish_reason: {:?}", response.finish_reason);
    for part in response.output.content {
        if let agent_toolkit::ContentPart::Text { text } = part {
            println!("assistant: {text}");
        }
    }

    Ok(())
}
