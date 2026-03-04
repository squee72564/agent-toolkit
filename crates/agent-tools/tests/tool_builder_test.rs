use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agent_core::types::ToolDefinition;
use agent_tools::{
    Tool, ToolBuilder, ToolBuilderError, ToolOutput, ToolRegistry, ToolRegistryError,
};
use serde_json::json;

fn strict_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

#[tokio::test]
async fn builder_construction_and_tool_definitions_work() {
    let tool = ToolBuilder::new()
        .name("search")
        .description("Search for documents")
        .schema(strict_schema())
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build()
        .expect("builder should produce a tool");

    assert_eq!(tool.name(), "search");
    assert_eq!(tool.description(), Some("Search for documents"));
    assert_eq!(tool.input_schema(), strict_schema());

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("schema should compile");
    let definitions = registry.tool_definitions();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].name, "search");
    assert_eq!(
        definitions[0].description.as_deref(),
        Some("Search for documents")
    );
    assert_eq!(definitions[0].parameters_schema, strict_schema());
}

#[test]
fn builder_reports_schema_compile_failure() {
    let result = ToolBuilder::new()
        .name("search")
        .schema(json!({
            "type": "object",
            "properties": 12
        }))
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build();

    match result {
        Err(ToolBuilderError::InvalidSchema { .. }) => {}
        Err(other) => panic!("unexpected builder error: {other}"),
        Ok(_) => panic!("invalid schema should fail at build"),
    }
}

#[tokio::test]
async fn args_validation_failure_blocks_execution() {
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let calls = execute_calls.clone();

    let tool = ToolBuilder::new()
        .name("search")
        .schema(strict_schema())
        .handler(move |args| {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(ToolOutput { content: args })
            }
        })
        .build()
        .expect("builder should produce a tool");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("schema should compile");

    let error = registry
        .execute_validated("search", json!({}))
        .await
        .expect_err("missing required field should fail");

    assert!(matches!(error, ToolRegistryError::InvalidArgs { .. }));
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execution_success_with_valid_args() {
    let tool = ToolBuilder::new()
        .name("search")
        .schema(strict_schema())
        .handler(|args| async move {
            Ok(ToolOutput {
                content: json!({
                    "echo": args["query"]
                }),
            })
        })
        .build()
        .expect("builder should produce a tool");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("schema should compile");

    let output = registry
        .execute_validated("search", json!({ "query": "rust" }))
        .await
        .expect("valid args should execute");

    assert_eq!(output.content, json!({ "echo": "rust" }));
}

#[test]
fn from_definition_pipeline_builds_tool() {
    let definition = ToolDefinition {
        name: "lookup".to_string(),
        description: Some("Lookup data".to_string()),
        parameters_schema: strict_schema(),
    };

    let tool = ToolBuilder::from_definition(definition)
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build()
        .expect("from_definition should be compatible with builder flow");

    assert_eq!(tool.name(), "lookup");
    assert_eq!(tool.description(), Some("Lookup data"));
    assert_eq!(tool.input_schema(), strict_schema());
}
