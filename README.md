# agent_toolkit (WIP)

Minimal Rust workspace for providing basic agent building primitives.

This is an educational repository and is not intended to be used for production code.

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
    let input = convo
        .to_input()
        .with_tools(registry.tool_definitions())
        .with_tool_choice(ToolChoice::Auto);

    let response = client.messages().create(input).await?;

    for part in response.output.content {
        match part {
            ContentPart::Text { text } => {
                println!("assistant: {text}");
                convo.push_assistant_text(text);
            }
            ContentPart::ToolCall { tool_call } => {
                println!("tool call: {} {}", tool_call.name, tool_call.arguments_json);
                convo.push_assistant_tool_call(
                    tool_call.id.clone(),
                    tool_call.name.clone(),
                    tool_call.arguments_json.clone(),
                );
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

Typed tool authoring (auto-schema from Rust types) is also supported:

```rust
use agent_toolkit::tools::{ToolBuilder, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

let tool = ToolBuilder::new()
    .name("get_weather_typed")
    .description("Get current weather by city")
    .typed_handler(|args: WeatherArgs| async move {
        Ok::<WeatherOut, ToolError>(WeatherOut {
            city: args.city,
            temp_f: 67,
            conditions: "sunny".to_string(),
        })
    })
    .build()?;
```

Schema precedence for `ToolBuilder`:
- Default for `.typed_handler(...)`: input schema is derived from the `TArgs` type.
- Override: if `.schema(...)` is called after `.typed_handler(...)`, the manual schema is used.

Use raw `.handler(|serde_json::Value| ...)` when you need dynamic payload handling or the lowest-overhead local hot path.

### Rule-based routing fallback

```rust
use agent_toolkit::{
    AgentToolkit, FallbackMode, FallbackPolicy, FallbackRule, MessageCreateInput, ProviderConfig,
    ProviderId, SendOptions, Target,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let toolkit = AgentToolkit::builder()
        .with_openai(ProviderConfig::new(std::env::var("OPENAI_API_KEY")?).with_default_model("gpt-5-mini"))
        .with_openrouter(ProviderConfig::new(std::env::var("OPENROUTER_API_KEY")?))
        .build()?;

    let fallback_policy = FallbackPolicy::new(vec![Target::new(ProviderId::OpenRouter)])
        .with_mode(FallbackMode::RulesOnly)
        .with_rule(FallbackRule::retry_on_status(429))
        .with_rule(FallbackRule::retry_on_provider_code("rate_limit_exceeded"));

    let response = toolkit
        .messages()
        .create(
            MessageCreateInput::user("Write one sentence about Rust."),
            SendOptions::for_target(Target::new(ProviderId::OpenAi))
                .with_fallback_policy(fallback_policy),
        )
        .await?;

    println!("model: {}", response.model);
    Ok(())
}
```

### Observability hooks

```rust
use std::sync::Arc;

use agent_toolkit::{
    openai, AgentToolkit, MessageCreateInput, ProviderConfig, RequestEndEvent, RequestStartEvent,
    RuntimeObserver, SendOptions, Target, ProviderId,
};

#[derive(Debug)]
struct PrintObserver;

impl RuntimeObserver for PrintObserver {
    fn on_request_start(&self, event: &RequestStartEvent) {
        println!("request started: provider={:?} model={:?}", event.provider, event.model);
    }

    fn on_request_end(&self, event: &RequestEndEvent) {
        println!(
            "request ended: status={:?} error_kind={:?}",
            event.status_code, event.error_kind
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observer: Arc<dyn RuntimeObserver> = Arc::new(PrintObserver);

    let client = openai()
        .api_key(std::env::var("OPENAI_API_KEY")?)
        .default_model("gpt-5-mini")
        .observer(observer.clone())
        .build()?;

    let _ = client
        .messages()
        .create(MessageCreateInput::user("Say hi in five words."))
        .await?;

    let toolkit = AgentToolkit::builder()
        .with_openai(
            ProviderConfig::new(std::env::var("OPENAI_API_KEY")?).with_default_model("gpt-5-mini"),
        )
        .observer(observer.clone())
        .build()?;

    let per_call_observer: Arc<dyn RuntimeObserver> = Arc::new(PrintObserver);
    let _ = toolkit
        .messages()
        .create(
            MessageCreateInput::user("One sentence about Rust."),
            SendOptions::for_target(Target::new(ProviderId::OpenAi))
                .with_observer(per_call_observer),
        )
        .await?;

    Ok(())
}
```

Observer precedence is `SendOptions::with_observer(...)` > `AgentToolkit::builder().observer(...)` > provider-client builder `.observer(...)`. Observer callback panics are isolated and never propagate into request results.

## Workspace Layout

```text
.
├── AGENTS.md
├── CONTRIBUTING.md
├── Cargo.lock
├── Cargo.toml
├── README.md
└── crates
    ├── agent
    │   ├── Cargo.toml
    │   └── src
    │       └── lib.rs
    ├── agent-core
    │   ├── Cargo.toml
    │   └── src
    │       ├── lib.rs
    │       └── types.rs
    ├── agent-providers
    │   ├── Cargo.toml
    │   └── src
    │       ├── adapter
    │       ├── adapter.rs
    │       ├── anthropic_spec
    │       │   ├── decode.rs
    │       │   ├── encode.rs
    │       │   ├── mod.rs
    │       │   └── schema_rules.rs
    │       ├── error.rs
    │       ├── lib.rs
    │       ├── openai_spec
    │       │   ├── decode.rs
    │       │   ├── encode.rs
    │       │   ├── mod.rs
    │       │   └── schema_rules.rs
    │       ├── platform
    │       │   ├── anthropic
    │       │   │   ├── mod.rs
    │       │   │   └── translator.rs
    │       │   ├── mod.rs
    │       │   ├── openai
    │       │   │   ├── mod.rs
    │       │   │   └── translator.rs
    │       │   └── openrouter
    │       │       ├── mod.rs
    │       │       └── translator.rs
    │       └── translator_contract.rs
    ├── agent-runtime
    │   ├── Cargo.toml
    │   └── src
    │       ├── agent_toolkit.rs
    │       ├── base_client_builder.rs
    │       ├── clients
    │       │   ├── anthropic.rs
    │       │   ├── mod.rs
    │       │   ├── openai.rs
    │       │   └── openrouter.rs
    │       ├── conversation.rs
    │       ├── fallback.rs
    │       ├── lib.rs
    │       ├── message_create_input.rs
    │       ├── direct_messages_api.rs
    │       ├── observer.rs
    │       ├── provider_client.rs
    │       ├── provider_config.rs
    │       ├── provider_runtime.rs
    │       ├── routed_messages_api.rs
    │       ├── runtime_error.rs
    │       ├── send_options.rs
    │       ├── target.rs
    │       └── types.rs
    ├── agent-tools
    │   ├── Cargo.toml
    │   └── src
    │       ├── builder.rs
    │       ├── lib.rs
    │       ├── registry.rs
    │       ├── runtime.rs
    │       ├── schema.rs
    │       └── tool.rs
    └── agent-transport
        ├── Cargo.toml
        └── src
            ├── http
            │   ├── builder.rs
            │   ├── mod.rs
            │   ├── retry_policy.rs
            │   └── transport.rs
            └── lib.rs
```

## Crates

- `agent` (`agent_toolkit`): facade crate with public re-exports for core, runtime, providers, transport, and tools.
- `agent-core`: provider-agnostic domain types and traits shared across crates, including canonical `ProviderId`.
- `agent-providers`: provider-specific encode/decode/spec logic, static `ProviderAdapter` lookup boundary, and fixture datasets for validation tests.
- `agent-runtime`: high-level clients (`openai()`, `anthropic()`, `openrouter()`), toolkit routing/fallback orchestration, and unified adapter-driven execution flow.
- `agent-transport`: HTTP transport implementation with retry support, auth/header handling, and JSON request/response helpers.
- `agent-tools`: lightweight tool trait and registry primitives for tool integration.

## TODO 
- built-in tool-execution loop (agent-runner) over Response::ToolCalls.
- streaming responses API (token/tool-call deltas).
- preserve and expose reasoning/thinking content instead of dropping it.
- multimodal input support (images/files in message content)

## Release-readiness quality gates

This workspace uses deterministic release-readiness gates in CI:

1. `cargo check --workspace --all-targets --locked`
2. `cargo fmt --all -- --check`
3. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
4. `cargo clippy --workspace --lib --all-features -- -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic`
5. `cargo test --workspace --all-targets --all-features`
6. `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --document-private-items`

`clippy::unwrap_used`, `clippy::expect_used`, and `clippy::panic` are intentionally enforced on non-test targets in this milestone. Existing test code remains outside full migration scope for now.

## Deterministic vs live tests

The default CI quality path is deterministic and does not make outbound provider calls.

Live provider tests are opt-in and only run when explicitly requested in workflow dispatch or when `RUN_LIVE_TESTS=true` is configured in repository variables. The live test contract requires:

- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `OPENROUTER_API_KEY`

If credentials are missing, the `live_tests` job exits with a clear deterministic skip message.

## Toolchain and compatibility policy

- Toolchain source of truth: `rust-toolchain.toml` (`1.93.0`, with `rustfmt` + `clippy`).
- Workspace compatibility floor: `rust-version = "1.88"`.
- Workspace lint policy is centralized in root `Cargo.toml` and inherited in all crates via `[lints] workspace = true`.

## Publish-readiness metadata

Workspace crate metadata is normalized for release readiness (license, repository/homepage/documentation, readme, keywords, categories, descriptions).

Maintainers can validate publish readiness per crate using:

```bash
cargo publish --dry-run -p <crate-name>
```
