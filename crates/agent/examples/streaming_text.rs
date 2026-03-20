use std::env;
use std::io::{self, Write};

use agent_toolkit::prelude::{MessageCreateInput, openai};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let client = openai()
        .api_key(env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .build()?;

    let mut stream = client
        .streaming()
        .create(MessageCreateInput::user(
            "Reply with two short sentences about Rust traits.",
        ))
        .await?
        .into_text_stream();

    while let Some(chunk) = stream.next().await {
        print!("{}", chunk?);
        io::stdout().flush()?;
    }
    println!();

    let completion = stream.finish().await?;
    println!("model: {}", completion.response.model);
    println!("provider: {:?}", completion.meta.selected_provider_kind);

    Ok(())
}
