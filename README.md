# agent_toolkit

Minimal Rust workspace for provider-agnostic agent infrastructure.

## High-level Usage

### Basic OpenAI request

```rust
use agent_toolkit::{openai, MessageCreateInput};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = openai()
        .api_key(std::env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .build()?;

    let response = client
        .messages()
        .create(MessageCreateInput::user("Write one sentence about Rust."))
        .await?;

    println!("model: {}", response.model);
    println!("finish_reason: {:?}", response.finish_reason);
    Ok(())
}
```

### Using `Conversation` state

```rust
use agent_toolkit::{openai, Conversation};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = openai()
        .api_key(std::env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .build()?;

    let mut convo = Conversation::with_user_text("What is ownership in Rust?");
    let response = client.messages().create(convo.to_input()).await?;

    // You control history updates in app code.
    convo.push_assistant_text(format!("{:?}", response.output.content));
    Ok(())
}
```

### Tool-enabled request with `Conversation` and `ToolRegistry`

```rust
use agent_toolkit::{openai, ContentPart, Conversation, ToolChoice};
use agent_toolkit::tools::{ToolBuilder, ToolRegistry, ToolOutput};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = openai()
        .api_key(std::env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .build()?;

    let mut registry = ToolRegistry::new();
    let weather_tool = ToolBuilder::new()
        .name("get_weather")
        .description("Get current weather by city")
        .schema(json!({
            "type": "object",
            "properties": {
                "city": { "type": "string" }
            },
            "required": ["city"],
            "additionalProperties": false
        }))
        .handler(|args| async move {
            let city = args["city"].as_str().unwrap_or("unknown");
            Ok(ToolOutput {
                content: json!({
                    "city": city,
                    "temp_f": 67,
                    "conditions": "sunny"
                }),
            })
        })
        .build()?;
    registry.register_validated(weather_tool)?;

    let mut convo = Conversation::with_user_text("What is weather in SF?");
    let mut input = convo.to_input();
    input.tools = registry.tool_definitions();
    input.tool_choice = ToolChoice::Auto;

    let response = client.messages().create(input).await?;

    for part in response.output.content {
        match part {
            ContentPart::Text { text } => {
                println!("assistant: {text}");
                convo.push_assistant_text(text);
            }
            ContentPart::ToolCall { tool_call } => {
                println!("tool call: {} {}", tool_call.name, tool_call.arguments_json);
                let output = registry
                    .execute_validated(&tool_call.name, tool_call.arguments_json)
                    .await?;
                convo.push_tool_result_json(tool_call.id, output.content);
            }
            ContentPart::ToolResult { .. } => {}
        }
    }

    Ok(())
}
```
## Workspace Layout

```text
.
├── AGENTS.md
├── Cargo.lock
├── Cargo.toml
├── crates
│   ├── agent
│   │   ├── Cargo.toml
│   │   └── src
│   │       ├── lib.rs
│   │       └── test.rs
│   ├── agent-core
│   │   ├── Cargo.toml
│   │   ├── src
│   │   │   ├── error
│   │   │   │   └── mod.rs
│   │   │   ├── lib.rs
│   │   │   ├── traits
│   │   │   │   └── mod.rs
│   │   │   └── types
│   │   │       └── mod.rs
│   │   └── tests
│   │       └── message_helpers_test.rs
│   ├── agent-providers
│   │   ├── Cargo.toml
│   │   ├── data
│   │   │   ├── anthropic
│   │   │   ├── openai
│   │   │   └── openrouter
│   │   └── src
│   │       ├── adapter
│   │       │   └── test.rs
│   │       ├── adapter.rs
│   │       ├── anthropic_spec
│   │       │   ├── decode.rs
│   │       │   ├── encode.rs
│   │       │   ├── mod.rs
│   │       │   ├── schema_rules.rs
│   │       │   └── test.rs
│   │       ├── error.rs
│   │       ├── lib.rs
│   │       ├── openai_spec
│   │       │   ├── decode.rs
│   │       │   ├── encode.rs
│   │       │   ├── mod.rs
│   │       │   ├── schema_rules.rs
│   │       │   └── test.rs
│   │       ├── platform
│   │       │   ├── anthropic
│   │       │   │   ├── fixtures_test.rs
│   │       │   │   ├── mod.rs
│   │       │   │   ├── test.rs
│   │       │   │   └── translator.rs
│   │       │   ├── mod.rs
│   │       │   ├── openai
│   │       │   │   ├── fixtures_test.rs
│   │       │   │   ├── mod.rs
│   │       │   │   ├── test.rs
│   │       │   │   └── translator.rs
│   │       │   ├── openrouter
│   │       │   │   ├── fixtures_test.rs
│   │       │   │   ├── mod.rs
│   │       │   │   ├── test.rs
│   │       │   │   └── translator.rs
│   │       │   └── test_fixtures.rs
│   │       └── translator_contract.rs
│   ├── agent-runtime
│   │   ├── Cargo.toml
│   │   └── src
│   │       ├── lib.rs
│   │       └── test.rs
│   ├── agent-tools
│   │   ├── Cargo.toml
│   │   ├── src
│   │   │   ├── builder.rs
│   │   │   ├── lib.rs
│   │   │   └── schema.rs
│   │   └── tests
│   │       ├── registry_test.rs
│   │       ├── schema_test.rs
│   │       └── tool_builder_test.rs
│   └── agent-transport
│       ├── Cargo.toml
│       └── src
│           ├── http
│           │   └── mod.rs
│           └── lib.rs
├── README.md
└── tests
```

## Crates

- `agent` (`agent_toolkit`): facade crate with public re-exports for core, runtime, providers, transport, and tools.
- `agent-core`: provider-agnostic domain types and traits shared across crates, including canonical `ProviderId`.
- `agent-providers`: provider-specific encode/decode/spec logic, static `ProviderAdapter` lookup boundary, and fixture datasets for validation tests.
- `agent-runtime`: high-level clients (`openai()`, `anthropic()`, `openrouter()`), toolkit routing/fallback orchestration, and unified adapter-driven execution flow.
- `agent-transport`: HTTP transport implementation with retry support, auth/header handling, and JSON request/response helpers.
- `agent-tools`: lightweight tool trait and registry primitives for tool integration.
