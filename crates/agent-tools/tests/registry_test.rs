use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agent_tools::{Tool, ToolOutput, ToolRegistry, ToolRegistryError};
use async_trait::async_trait;
use serde_json::{Value, json};

struct TestTool {
    name: String,
    schema: Value,
    input_schema_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
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

    async fn execute(&self, args: Value) -> Result<ToolOutput, agent_tools::ToolError> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput { content: args })
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

#[test]
fn register_and_get_behavior_is_validated() {
    let mut registry = ToolRegistry::new();
    let input_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let tool = build_tool(
        "legacy",
        json!({"type": "object"}),
        input_calls,
        execute_calls,
    );

    registry.register(tool).expect("schema should be validated");

    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
    assert!(registry.get("legacy").is_some());
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
}

#[test]
fn register_rejects_duplicate_names_without_overwriting() {
    let mut registry = ToolRegistry::new();
    let first_input_calls = Arc::new(AtomicUsize::new(0));
    let first_execute_calls = Arc::new(AtomicUsize::new(0));
    registry
        .register(build_tool(
            "search",
            json!({"type": "object"}),
            first_input_calls.clone(),
            first_execute_calls,
        ))
        .expect("first registration should succeed");

    let second_input_calls = Arc::new(AtomicUsize::new(0));
    let second_execute_calls = Arc::new(AtomicUsize::new(0));
    let error = registry
        .register(build_tool(
            "search",
            json!({"type": "object"}),
            second_input_calls.clone(),
            second_execute_calls,
        ))
        .expect_err("duplicate registration should fail");

    assert!(matches!(
        error,
        ToolRegistryError::DuplicateName { name } if name == "search"
    ));
    assert_eq!(registry.len(), 1);
    assert_eq!(first_input_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_input_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn tool_definitions_returns_sorted_provider_ready_definitions() {
    let mut registry = ToolRegistry::new();
    let first_input_calls = Arc::new(AtomicUsize::new(0));
    let first_execute_calls = Arc::new(AtomicUsize::new(0));
    registry
        .register(build_tool(
            "zeta",
            json!({"type": "object"}),
            first_input_calls.clone(),
            first_execute_calls,
        ))
        .expect("registration should succeed");
    let second_input_calls = Arc::new(AtomicUsize::new(0));
    let second_execute_calls = Arc::new(AtomicUsize::new(0));
    registry
        .register(build_tool(
            "alpha",
            json!({"type": "object"}),
            second_input_calls.clone(),
            second_execute_calls,
        ))
        .expect("registration should succeed");

    let definitions = registry.tool_definitions();

    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[0].name, "alpha");
    assert_eq!(definitions[1].name, "zeta");
    assert_eq!(definitions[0].description.as_deref(), Some("test tool"));
    assert_eq!(definitions[1].parameters_schema, json!({"type": "object"}));
    assert_eq!(first_input_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_input_calls.load(Ordering::SeqCst), 1);
}
