use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agent_tools::{
    Tool, ToolBuilder, ToolError, ToolOutput, ToolRegistry, ToolRuntime, ToolRuntimeError,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

struct TestTool {
    name: String,
    schema: Value,
    input_schema_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

struct FailingTool {
    name: String,
    schema: Value,
}

#[async_trait]
impl Tool for TestTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("test tool")
    }

    fn input_schema(&self) -> Value {
        self.input_schema_calls.fetch_add(1, Ordering::SeqCst);
        self.schema.clone()
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput { content: args })
    }
}

#[async_trait]
impl Tool for FailingTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("failing tool")
    }

    fn input_schema(&self) -> Value {
        self.schema.clone()
    }

    async fn execute(&self, _args: Value) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Execution("simulated failure".to_string()))
    }
}

fn build_tool(
    name: &str,
    schema: Value,
    input_schema_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
) -> TestTool {
    TestTool {
        name: name.to_string(),
        schema,
        input_schema_calls,
        execute_calls,
    }
}

fn build_failing_tool(name: &str, schema: Value) -> FailingTool {
    FailingTool {
        name: name.to_string(),
        schema,
    }
}

fn strict_schema(required_field: &str) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert(required_field.to_string(), json!({ "type": "integer" }));

    json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": [required_field],
        "additionalProperties": false
    })
}

#[test]
fn validation_uses_registered_schema() {
    let mut registry = ToolRegistry::new();
    let input_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let tool = build_tool(
        "search",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
        input_calls.clone(),
        execute_calls,
    );

    registry.register_validated(tool).expect("valid schema");
    assert_eq!(input_calls.load(Ordering::SeqCst), 1);
    let runtime = ToolRuntime::new(&registry);

    runtime
        .validate_call("search", &json!({"query": "rust"}))
        .expect("validation should pass");

    assert_eq!(input_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn validate_call_reports_unknown_tool() {
    let registry = ToolRegistry::new();
    let runtime = ToolRuntime::new(&registry);
    let error = runtime
        .validate_call("missing", &json!({}))
        .expect_err("should fail for unknown tool");

    assert!(matches!(
        error,
        ToolRuntimeError::UnknownTool { name } if name == "missing"
    ));
}

#[tokio::test]
async fn execute_reports_unknown_tool() {
    let registry = ToolRegistry::new();
    let runtime = ToolRuntime::new(&registry);
    let error = runtime
        .execute("missing", json!({}))
        .await
        .expect_err("should fail for unknown tool");

    assert!(matches!(
        error,
        ToolRuntimeError::UnknownTool { name } if name == "missing"
    ));
}

#[tokio::test]
async fn execute_blocks_execution_when_args_are_invalid() {
    let mut registry = ToolRegistry::new();
    let input_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let tool = build_tool(
        "search",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
        input_calls,
        execute_calls.clone(),
    );

    registry.register_validated(tool).expect("valid schema");
    let runtime = ToolRuntime::new(&registry);
    let error = runtime
        .execute("search", json!({}))
        .await
        .expect_err("invalid args should fail");

    assert!(matches!(error, ToolRuntimeError::InvalidArgs { .. }));
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execute_runs_tool_for_valid_args() {
    let mut registry = ToolRegistry::new();
    let input_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let tool = build_tool(
        "search",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
        input_calls,
        execute_calls.clone(),
    );

    registry.register_validated(tool).expect("valid schema");
    let runtime = ToolRuntime::new(&registry);
    let output = runtime
        .execute("search", json!({ "query": "rust" }))
        .await
        .expect("valid args should execute tool");

    assert_eq!(output.content, json!({ "query": "rust" }));
    assert_eq!(execute_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn execute_wraps_tool_execution_errors() {
    let mut registry = ToolRegistry::new();
    registry
        .register_validated(build_failing_tool(
            "search",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ))
        .expect("schema should compile");

    let runtime = ToolRuntime::new(&registry);
    let error = runtime
        .execute("search", json!({}))
        .await
        .expect_err("execution failure should be wrapped");

    assert!(matches!(
        error,
        ToolRuntimeError::Execution { ref name, ref source }
            if name == "search"
                && source.to_string() == "tool execution failed: simulated failure"
    ));
}

#[test]
fn validation_uses_cached_schema_on_each_call() {
    let mut registry = ToolRegistry::new();
    let input_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    registry
        .register(build_tool(
            "search",
            strict_schema("query"),
            input_calls.clone(),
            execute_calls,
        ))
        .expect("registration should compile schema once");
    let runtime = ToolRuntime::new(&registry);

    runtime
        .validate_call("search", &json!({ "query": 1 }))
        .expect("first call should compile schema and pass validation");
    runtime
        .validate_call("search", &json!({ "query": 2 }))
        .expect("second call should compile schema and pass validation");

    assert_eq!(input_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn register_rejects_duplicate_names_before_runtime_validation() {
    let mut registry = ToolRegistry::new();
    let old_input_calls = Arc::new(AtomicUsize::new(0));
    let old_execute_calls = Arc::new(AtomicUsize::new(0));
    let old_tool = build_tool(
        "search",
        strict_schema("a"),
        old_input_calls,
        old_execute_calls,
    );
    registry
        .register_validated(old_tool)
        .expect("first schema should compile");

    let new_input_calls = Arc::new(AtomicUsize::new(0));
    let new_execute_calls = Arc::new(AtomicUsize::new(0));
    let new_tool = build_tool(
        "search",
        strict_schema("b"),
        new_input_calls.clone(),
        new_execute_calls,
    );
    let error = registry
        .register(new_tool)
        .expect_err("duplicate registration should fail");

    assert!(matches!(
        error,
        agent_tools::ToolRegistryError::DuplicateName { name } if name == "search"
    ));
    let runtime = ToolRuntime::new(&registry);
    runtime
        .validate_call("search", &json!({"a": 1}))
        .expect("first schema should remain active");
    assert_eq!(new_input_calls.load(Ordering::SeqCst), 0);
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
async fn non_object_args_block_execution() {
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let calls = execute_calls.clone();

    let tool = ToolBuilder::new()
        .name("search")
        .schema(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"],
            "additionalProperties": false
        }))
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
    let runtime = ToolRuntime::new(&registry);

    let error = runtime
        .execute("search", json!("not-an-object"))
        .await
        .expect_err("non-object args should fail");

    match error {
        ToolRuntimeError::InvalidArgs { name, source } => {
            assert_eq!(name, "search");
            assert_eq!(source.to_string(), "tool arguments must be a JSON object");
        }
        other => panic!("expected InvalidArgs error, got {other}"),
    }
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
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
    let runtime = ToolRuntime::new(&registry);

    let error = runtime
        .execute("echo", json!({ "query": 42 }))
        .await
        .expect_err("schema validation should fail for wrong field type");

    assert!(matches!(error, ToolRuntimeError::InvalidArgs { .. }));
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execution_failure_surfaces_tool_name_and_source() {
    let tool = ToolBuilder::new()
        .name("search")
        .schema(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"],
            "additionalProperties": false
        }))
        .handler(|_| async move { Err(ToolError::Execution("boom".to_string())) })
        .build()
        .expect("builder should produce a tool");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("schema should compile");
    let runtime = ToolRuntime::new(&registry);

    let error = runtime
        .execute("search", json!({ "query": "rust" }))
        .await
        .expect_err("execution failure should surface as runtime error");

    match error {
        ToolRuntimeError::Execution { name, source } => {
            assert_eq!(name, "search");
            assert_eq!(source.to_string(), "tool execution failed: boom");
        }
        other => panic!("expected Execution error, got {other}"),
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DecodeStrictArgs {
    #[serde(deserialize_with = "deserialize_non_empty")]
    query: String,
}

#[derive(serde::Serialize)]
struct DecodeStrictOut {
    query: String,
}

#[tokio::test]
async fn execute_maps_typed_input_decode_failures_to_invalid_args() {
    let tool = ToolBuilder::new()
        .name("search")
        .typed_handler(
            |args: DecodeStrictArgs| async move { Ok(DecodeStrictOut { query: args.query }) },
        )
        .build()
        .expect("typed tool should build");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(tool)
        .expect("typed schema should compile");
    let runtime = ToolRuntime::new(&registry);

    let error = runtime
        .execute("search", json!({"query": ""}))
        .await
        .expect_err("decode should fail for empty query");

    match error {
        ToolRuntimeError::InvalidArgs { name, source } => {
            assert_eq!(name, "search");
            assert!(
                source
                    .to_string()
                    .starts_with("tool 'search' input decode failed:")
            );
            match source {
                agent_tools::ToolArgsValidationError::ValidationFailed { issues, .. } => {
                    assert_eq!(issues.len(), 1);
                    assert_eq!(issues[0].instance_path, "$");
                    assert!(issues[0].message.contains("query must not be empty"));
                }
                other => panic!("expected validation failure details, got {other}"),
            }
        }
        other => panic!("expected invalid args error, got {other}"),
    }
}

fn deserialize_non_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom("query must not be empty"));
    }
    Ok(value)
}

#[tokio::test]
async fn typed_and_raw_tools_can_mix_in_one_registry() {
    let raw_tool = ToolBuilder::new()
        .name("raw_echo")
        .schema(json!({
            "type": "object",
            "properties": { "value": { "type": "string" } },
            "required": ["value"],
            "additionalProperties": false
        }))
        .handler(|args| async move { Ok(ToolOutput { content: args }) })
        .build()
        .expect("raw tool should build");

    #[derive(Deserialize, JsonSchema)]
    struct TypedArgs {
        value: String,
    }

    #[derive(serde::Serialize)]
    struct TypedOut {
        wrapped: String,
    }

    let typed_tool = ToolBuilder::new()
        .name("typed_echo")
        .typed_handler(|args: TypedArgs| async move {
            Ok(TypedOut {
                wrapped: format!("typed:{}", args.value),
            })
        })
        .build()
        .expect("typed tool should build");

    let mut registry = ToolRegistry::new();
    registry
        .register_validated(raw_tool)
        .expect("raw schema should compile");
    registry
        .register_validated(typed_tool)
        .expect("typed schema should compile");
    let runtime = ToolRuntime::new(&registry);

    let raw_output = runtime
        .execute("raw_echo", json!({"value":"hi"}))
        .await
        .expect("raw tool should execute");
    assert_eq!(raw_output.content, json!({"value":"hi"}));

    let typed_output = runtime
        .execute("typed_echo", json!({"value":"hi"}))
        .await
        .expect("typed tool should execute");
    assert_eq!(typed_output.content, json!({"wrapped":"typed:hi"}));
}
