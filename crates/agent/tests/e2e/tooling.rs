use std::error::Error as StdError;
use std::fmt;

use agent_toolkit::tools::{ToolBuilder, ToolOutput, ToolRegistry, ToolRuntime, ToolRuntimeError};
use agent_toolkit::{ContentPart, Conversation, MessageCreateInput, Response, ToolChoice};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug)]
pub enum ToolLoopError {
    Runtime(ToolRuntimeError),
}

impl fmt::Display for ToolLoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => write!(f, "tool runtime error: {error}"),
        }
    }
}

impl StdError for ToolLoopError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
        }
    }
}

impl From<ToolRuntimeError> for ToolLoopError {
    fn from(value: ToolRuntimeError) -> Self {
        Self::Runtime(value)
    }
}

pub async fn orchestrate_tool_calls(
    response: &Response,
    conversation: &mut Conversation,
    registry: &ToolRegistry,
) -> Result<Option<MessageCreateInput>, ToolLoopError> {
    let mut tool_calls = Vec::new();

    for part in &response.output.content {
        if let ContentPart::ToolCall { tool_call } = part {
            tool_calls.push(tool_call.clone());
        }
    }

    if tool_calls.is_empty() {
        return Ok(None);
    }
    let runtime = ToolRuntime::new(registry);

    for tool_call in tool_calls {
        conversation.push_assistant_tool_call(
            tool_call.id.clone(),
            tool_call.name.clone(),
            tool_call.arguments_json.clone(),
        );

        let output = runtime
            .execute(&tool_call.name, tool_call.arguments_json)
            .await?;

        conversation.push_tool_result_json(tool_call.id, output.content);
    }

    Ok(Some(conversation.to_input()))
}

pub fn build_registry_with_raw_and_typed_tools() -> ToolRegistry {
    let raw_tool = ToolBuilder::new()
        .name("raw_echo")
        .description("Raw echo tool")
        .schema(json!({
            "type": "object",
            "properties": {
                "value": { "type": "string" }
            },
            "required": ["value"],
            "additionalProperties": false
        }))
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build()
        .expect("raw tool should build");

    let typed_tool = ToolBuilder::new()
        .name("typed_echo")
        .description("Typed echo tool")
        .typed_handler(|args: TypedEchoArgs| async move {
            Ok(TypedEchoOut {
                wrapped: format!("typed:{}", args.value),
            })
        })
        .build()
        .expect("typed tool should build");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(raw_tool)
        .expect("raw tool schema should compile");
    registry
        .register_validated(typed_tool)
        .expect("typed tool schema should compile");
    registry
        .register_validated(
            ToolBuilder::new()
                .name("get_weather")
                .description("Fixture-compatible weather tool")
                .schema(json!({
                    "type": "object",
                    "properties": {
                        "city": { "type": "string" }
                    },
                    "required": ["city"],
                    "additionalProperties": true
                }))
                .handler(|args| async move {
                    Ok(ToolOutput {
                        content: json!({
                            "ok": true,
                            "echo": args
                        }),
                    })
                })
                .build()
                .expect("get_weather tool should build"),
        )
        .expect("get_weather schema should compile");
    registry
}

pub fn tool_enabled_input(
    input: MessageCreateInput,
    registry: &ToolRegistry,
) -> MessageCreateInput {
    input
        .with_tools(registry.tool_definitions())
        .with_tool_choice(ToolChoice::Auto)
}

pub fn build_raw_echo_only_registry() -> ToolRegistry {
    let raw_tool = ToolBuilder::new()
        .name("raw_echo")
        .description("Raw echo tool")
        .schema(json!({
            "type": "object",
            "properties": {
                "value": { "type": "string" }
            },
            "required": ["value"],
            "additionalProperties": false
        }))
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build()
        .expect("raw tool should build");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(raw_tool)
        .expect("raw tool schema should compile");
    registry
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TypedEchoArgs {
    value: String,
}

#[derive(Debug, Serialize)]
struct TypedEchoOut {
    wrapped: String,
}

pub async fn execute_raw_echo(
    registry: &ToolRegistry,
    value: serde_json::Value,
) -> Result<ToolOutput, ToolRuntimeError> {
    ToolRuntime::new(registry).execute("raw_echo", value).await
}

pub async fn execute_typed_echo(
    registry: &ToolRegistry,
    value: serde_json::Value,
) -> Result<ToolOutput, ToolRuntimeError> {
    ToolRuntime::new(registry)
        .execute("typed_echo", value)
        .await
}
