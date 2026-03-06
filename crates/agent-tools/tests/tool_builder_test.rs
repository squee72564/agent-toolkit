use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agent_core::types::ToolDefinition;
use agent_tools::{
    Tool, ToolBuilder, ToolBuilderError, ToolError, ToolOutput, ToolRegistry, ToolRegistryError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, Serializer};
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

#[derive(Debug, Deserialize, JsonSchema)]
struct EchoArgs {
    query: String,
}

#[derive(Debug, Serialize)]
struct EchoOut {
    echo: String,
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

    assert!(matches!(
        result,
        Err(ToolBuilderError::InvalidSchema { .. })
    ));
}

#[test]
fn build_fails_when_name_is_missing() {
    let result = ToolBuilder::new()
        .schema(strict_schema())
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build();

    assert!(matches!(result, Err(ToolBuilderError::MissingName)));
}

#[test]
fn build_fails_when_name_is_blank() {
    let result = ToolBuilder::new()
        .name("   ")
        .schema(strict_schema())
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build();

    assert!(matches!(result, Err(ToolBuilderError::MissingName)));
}

#[test]
fn build_fails_when_schema_is_missing() {
    let result = ToolBuilder::new()
        .name("search")
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build();

    assert!(matches!(result, Err(ToolBuilderError::MissingSchema)));
}

#[test]
fn build_fails_when_handler_is_missing() {
    let result = ToolBuilder::new()
        .name("search")
        .schema(strict_schema())
        .build();

    assert!(matches!(result, Err(ToolBuilderError::MissingHandler)));
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

#[test]
fn from_definition_with_blank_name_fails_at_build() {
    let definition = ToolDefinition {
        name: " \t\n".to_string(),
        description: Some("Lookup data".to_string()),
        parameters_schema: strict_schema(),
    };

    let result = ToolBuilder::from_definition(definition)
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build();

    assert!(matches!(result, Err(ToolBuilderError::MissingName)));
}

#[test]
fn from_definition_with_invalid_schema_fails_at_build() {
    let definition = ToolDefinition {
        name: "lookup".to_string(),
        description: Some("Lookup data".to_string()),
        parameters_schema: json!({
            "type": "object",
            "properties": 12
        }),
    };

    let result = ToolBuilder::from_definition(definition)
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build();

    assert!(matches!(
        result,
        Err(ToolBuilderError::InvalidSchema { .. })
    ));
}

#[tokio::test]
async fn non_object_args_block_execution() {
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
        .execute_validated("search", json!("not-an-object"))
        .await
        .expect_err("non-object args should fail");

    match error {
        ToolRegistryError::InvalidArgs { name, source } => {
            assert_eq!(name, "search");
            assert_eq!(source.to_string(), "tool arguments must be a JSON object");
        }
        other => panic!("expected InvalidArgs error, got {other}"),
    }
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execution_failure_surfaces_tool_name_and_source() {
    let tool = ToolBuilder::new()
        .name("search")
        .schema(strict_schema())
        .handler(|_| async move { Err(ToolError::Execution("boom".to_string())) })
        .build()
        .expect("builder should produce a tool");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("schema should compile");

    let error = registry
        .execute_validated("search", json!({ "query": "rust" }))
        .await
        .expect_err("execution failure should surface as registry error");

    match error {
        ToolRegistryError::Execution { name, source } => {
            assert_eq!(name, "search");
            assert_eq!(source.to_string(), "tool execution failed: boom");
        }
        other => panic!("expected Execution error, got {other}"),
    }
}

#[tokio::test]
async fn typed_handler_round_trip_succeeds() {
    let tool = ToolBuilder::new()
        .name("echo")
        .typed_handler(|args: EchoArgs| async move {
            Ok(EchoOut {
                echo: args.query.to_uppercase(),
            })
        })
        .build()
        .expect("builder should produce a typed tool");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("schema should compile");

    let output = registry
        .execute_validated("echo", json!({ "query": "rust" }))
        .await
        .expect("typed execution should succeed");

    assert_eq!(output.content, json!({ "echo": "RUST" }));
}

#[test]
fn typed_handler_derives_object_schema_with_required_fields() {
    let tool = ToolBuilder::new()
        .name("echo")
        .typed_handler(|args: EchoArgs| async move { Ok(EchoOut { echo: args.query }) })
        .build()
        .expect("builder should produce a typed tool");

    let schema = tool.input_schema();
    assert_eq!(schema.get("type"), Some(&json!("object")));
    assert_eq!(
        schema.pointer("/properties/query/type"),
        Some(&json!("string"))
    );
    assert_eq!(schema.get("required"), Some(&json!(["query"])));
}

#[tokio::test]
async fn typed_handler_invalid_payload_blocks_handler_and_surfaces_invalid_args() {
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let calls = execute_calls.clone();

    let tool = ToolBuilder::new()
        .name("echo")
        .typed_handler(move |args: EchoArgs| {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(EchoOut { echo: args.query })
            }
        })
        .build()
        .expect("builder should produce a typed tool");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("schema should compile");

    let error = registry
        .execute_validated("echo", json!({ "query": 42 }))
        .await
        .expect_err("schema validation should fail for wrong field type");

    assert!(matches!(error, ToolRegistryError::InvalidArgs { .. }));
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
}

struct BrokenOutput;

impl Serialize for BrokenOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Err(serde::ser::Error::custom(format!(
            "forced failure at {:?}",
            serializer.is_human_readable()
        )))
    }
}

#[tokio::test]
async fn typed_handler_output_encode_failure_surfaces_as_execution() {
    let tool = ToolBuilder::new()
        .name("echo")
        .typed_handler(|_args: EchoArgs| async move { Ok(BrokenOutput) })
        .build()
        .expect("builder should produce a typed tool");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("schema should compile");

    let error = registry
        .execute_validated("echo", json!({ "query": "rust" }))
        .await
        .expect_err("output encode should fail");

    match error {
        ToolRegistryError::Execution { name, source } => {
            assert_eq!(name, "echo");
            assert!(matches!(source, ToolError::InvalidOutputEncode(_)));
        }
        other => panic!("expected execution error, got {other}"),
    }
}

#[test]
fn typed_handler_schema_can_be_overridden_with_manual_schema() {
    let manual_schema = strict_schema();

    let tool = ToolBuilder::new()
        .name("echo")
        .typed_handler(|args: EchoArgs| async move { Ok(EchoOut { echo: args.query }) })
        .schema(manual_schema.clone())
        .build()
        .expect("builder should produce a typed tool with manual schema override");

    assert_eq!(tool.input_schema(), manual_schema);
}

#[tokio::test]
async fn typed_vs_raw_overhead_timed_utility() {
    use std::time::Instant;

    const ITERATIONS: usize = 1_000;
    let args = json!({
        "query": "rust",
        "page": 2,
        "limit": 20,
        "filters": ["book", "crate", "guide"],
        "include_snippets": true,
    });

    #[derive(Debug, Deserialize, JsonSchema)]
    struct BenchArgs {
        query: String,
        page: u32,
        limit: u32,
        filters: Vec<String>,
        include_snippets: bool,
    }

    #[derive(Debug, Serialize)]
    struct BenchOut {
        query: String,
        score: u32,
    }

    let raw_tool = ToolBuilder::new()
        .name("raw")
        .schema(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "page": { "type": "integer" },
                "limit": { "type": "integer" },
                "filters": { "type": "array", "items": { "type": "string" } },
                "include_snippets": { "type": "boolean" }
            },
            "required": ["query", "page", "limit", "filters", "include_snippets"],
            "additionalProperties": false
        }))
        .handler(|value| async move {
            Ok(ToolOutput {
                content: json!({
                    "query": value["query"],
                    "score": 1_u32
                }),
            })
        })
        .build()
        .expect("raw tool should build");

    let typed_tool = ToolBuilder::new()
        .name("typed")
        .typed_handler(|value: BenchArgs| async move {
            let score = value.page
                + value.limit
                + value.filters.len() as u32
                + u32::from(value.include_snippets);
            Ok(BenchOut {
                query: value.query,
                score,
            })
        })
        .build()
        .expect("typed tool should build");

    let raw_start = Instant::now();
    for _ in 0..ITERATIONS {
        let output = raw_tool
            .execute(args.clone())
            .await
            .expect("raw execution should succeed");
        assert_eq!(output.content["score"], json!(1));
    }
    let raw_elapsed = raw_start.elapsed();

    let typed_start = Instant::now();
    for _ in 0..ITERATIONS {
        let output = typed_tool
            .execute(args.clone())
            .await
            .expect("typed execution should succeed");
        assert_eq!(output.content["query"], json!("rust"));
    }
    let typed_elapsed = typed_start.elapsed();

    let raw_per_call_ns = raw_elapsed.as_nanos() / ITERATIONS as u128;
    let typed_per_call_ns = typed_elapsed.as_nanos() / ITERATIONS as u128;

    assert!(raw_per_call_ns > 0);
    assert!(typed_per_call_ns > 0);
}
