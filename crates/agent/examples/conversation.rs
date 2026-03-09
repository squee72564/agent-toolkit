use std::env;

use agent_toolkit::{Conversation, openai};

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

    let client = openai()
        .api_key(env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .build()?;

    let mut conversation = Conversation::with_system_text("You are a concise Rust tutor.");
    conversation.push_user_text("What is ownership in Rust?");

    let first = client.messages().create(conversation.to_input()).await?;
    let first_text = response_text(&first.output.content);
    println!("assistant: {first_text}");
    conversation.push_assistant_text(first_text);

    conversation.push_user_text("Give one small example.");
    let second = client.messages().create(conversation.to_input()).await?;
    println!("assistant: {}", response_text(&second.output.content));

    Ok(())
}
