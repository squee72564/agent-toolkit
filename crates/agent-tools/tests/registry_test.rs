use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agent_tools::{Tool, ToolError, ToolOutput, ToolRegistry, ToolRegistryError};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};
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
fn register_and_get_behavior_is_unchanged() {
    let mut registry = ToolRegistry::new();
    let input_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let tool = build_tool(
        "legacy",
        json!({"type": "string"}),
        input_calls,
        execute_calls,
    );

    registry.register(tool);

    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
    assert!(registry.get("legacy").is_some());
}

#[test]
fn register_validated_compiles_once_and_uses_cached_schema() {
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

    registry
        .validate_call("search", &json!({"query": "rust"}))
        .expect("validation should pass");

    assert_eq!(input_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn validate_call_reports_unknown_tool() {
    let registry = ToolRegistry::new();
    let error = registry
        .validate_call("missing", &json!({}))
        .expect_err("should fail for unknown tool");

    assert!(matches!(
        error,
        ToolRegistryError::UnknownTool { name } if name == "missing"
    ));
}

#[tokio::test]
async fn execute_validated_reports_unknown_tool() {
    let registry = ToolRegistry::new();
    let error = registry
        .execute_validated("missing", json!({}))
        .await
        .expect_err("should fail for unknown tool");

    assert!(matches!(
        error,
        ToolRegistryError::UnknownTool { name } if name == "missing"
    ));
}

#[tokio::test]
async fn execute_validated_blocks_execution_when_args_are_invalid() {
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
    let error = registry
        .execute_validated("search", json!({}))
        .await
        .expect_err("invalid args should fail");

    assert!(matches!(error, ToolRegistryError::InvalidArgs { .. }));
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execute_validated_executes_tool_for_valid_args() {
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
    let output = registry
        .execute_validated("search", json!({ "query": "rust" }))
        .await
        .expect("valid args should execute tool");

    assert_eq!(output.content, json!({ "query": "rust" }));
    assert_eq!(execute_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn execute_validated_wraps_tool_execution_errors() {
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

    let error = registry
        .execute_validated("search", json!({}))
        .await
        .expect_err("execution failure should be wrapped");

    assert!(matches!(
        error,
        ToolRegistryError::Execution { ref name, ref source }
            if name == "search"
                && source.to_string() == "tool execution failed: simulated failure"
    ));
}

#[test]
fn register_validated_rejects_invalid_schema_without_registering_tool() {
    let mut registry = ToolRegistry::new();
    let input_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let invalid_tool = build_tool(
        "search",
        json!({
            "type": "object",
            "properties": 12
        }),
        input_calls,
        execute_calls,
    );

    let error = registry
        .register_validated(invalid_tool)
        .expect_err("invalid schema should fail registration");

    assert!(matches!(
        error,
        ToolRegistryError::InvalidSchema { name, .. } if name == "search"
    ));
    assert!(registry.is_empty());
    assert!(registry.get("search").is_none());
    assert!(matches!(
        registry.validate_call("search", &json!({})),
        Err(ToolRegistryError::UnknownTool { name }) if name == "search"
    ));
}

#[test]
fn validate_call_surfaces_invalid_schema_for_non_validated_registration() {
    let mut registry = ToolRegistry::new();
    let tool = build_tool(
        "search",
        json!({
            "type": "object",
            "properties": 12
        }),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    );
    registry.register(tool);

    let error = registry
        .validate_call("search", &json!({}))
        .expect_err("invalid schema should fail validation");

    assert!(matches!(
        error,
        ToolRegistryError::InvalidSchema { name, .. } if name == "search"
    ));
}

#[test]
fn non_validated_registration_recompiles_schema_on_each_validation_call() {
    let mut registry = ToolRegistry::new();
    let input_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    registry.register(build_tool(
        "search",
        strict_schema("query"),
        input_calls.clone(),
        execute_calls,
    ));

    registry
        .validate_call("search", &json!({ "query": 1 }))
        .expect("first call should compile schema and pass validation");
    registry
        .validate_call("search", &json!({ "query": 2 }))
        .expect("second call should compile schema and pass validation");

    assert_eq!(input_calls.load(Ordering::SeqCst), 2);
}

#[test]
fn register_clears_stale_compiled_schema_for_overwrites() {
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
    registry.register(new_tool);

    registry
        .validate_call("search", &json!({"b": 1}))
        .expect("new schema should be used");
    assert_eq!(new_input_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn register_validated_overwrite_updates_cached_schema() {
    let mut registry = ToolRegistry::new();
    let first_input_calls = Arc::new(AtomicUsize::new(0));
    let first_execute_calls = Arc::new(AtomicUsize::new(0));
    let first_tool = build_tool(
        "search",
        strict_schema("a"),
        first_input_calls,
        first_execute_calls,
    );
    registry
        .register_validated(first_tool)
        .expect("first schema should compile");

    let second_input_calls = Arc::new(AtomicUsize::new(0));
    let second_execute_calls = Arc::new(AtomicUsize::new(0));
    let second_tool = build_tool(
        "search",
        strict_schema("b"),
        second_input_calls,
        second_execute_calls,
    );
    registry
        .register_validated(second_tool)
        .expect("second schema should compile");

    registry
        .validate_call("search", &json!({"b": 1}))
        .expect("second schema should be active");
    let error = registry
        .validate_call("search", &json!({"a": 1}))
        .expect_err("old schema should no longer be active");

    assert!(matches!(error, ToolRegistryError::InvalidArgs { .. }));
}

#[test]
fn tool_definitions_returns_sorted_provider_ready_definitions() {
    let mut registry = ToolRegistry::new();
    let first_input_calls = Arc::new(AtomicUsize::new(0));
    let first_execute_calls = Arc::new(AtomicUsize::new(0));
    registry.register(build_tool(
        "zeta",
        json!({"type": "object"}),
        first_input_calls.clone(),
        first_execute_calls,
    ));
    let second_input_calls = Arc::new(AtomicUsize::new(0));
    let second_execute_calls = Arc::new(AtomicUsize::new(0));
    registry.register(build_tool(
        "alpha",
        json!({"type": "object"}),
        second_input_calls.clone(),
        second_execute_calls,
    ));

    let definitions = registry.tool_definitions();

    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[0].name, "alpha");
    assert_eq!(definitions[1].name, "zeta");
    assert_eq!(definitions[0].description.as_deref(), Some("test tool"));
    assert_eq!(definitions[1].parameters_schema, json!({"type": "object"}));
    assert_eq!(first_input_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_input_calls.load(Ordering::SeqCst), 1);
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
async fn execute_validated_maps_typed_input_decode_failures_to_invalid_args() {
    let tool = agent_tools::ToolBuilder::new()
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

    let error = registry
        .execute_validated("search", json!({"query": ""}))
        .await
        .expect_err("decode should fail for empty query");

    match error {
        ToolRegistryError::InvalidArgs { name, source } => {
            assert_eq!(name, "search");
            assert!(
                source
                    .to_string()
                    .starts_with("tool 'search' input decode failed:")
            );
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
    let raw_tool = agent_tools::ToolBuilder::new()
        .name("raw_echo")
        .schema(json!({
            "type": "object",
            "properties": { "value": { "type": "string" } },
            "required": ["value"],
            "additionalProperties": false
        }))
        .handler(|args| async move { Ok(agent_tools::ToolOutput { content: args }) })
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

    let typed_tool = agent_tools::ToolBuilder::new()
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

    let raw_output = registry
        .execute_validated("raw_echo", json!({"value":"hi"}))
        .await
        .expect("raw tool should execute");
    assert_eq!(raw_output.content, json!({"value":"hi"}));

    let typed_output = registry
        .execute_validated("typed_echo", json!({"value":"hi"}))
        .await
        .expect("typed tool should execute");
    assert_eq!(typed_output.content, json!({"wrapped":"typed:hi"}));
}
