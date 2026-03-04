use agent_core::types::ToolDefinition;
use agent_tools::{CompiledToolSchema, ToolArgsValidationError, ToolSchemaError};
use serde_json::json;

fn test_definition(schema: serde_json::Value) -> ToolDefinition {
    ToolDefinition {
        name: "search".to_string(),
        description: Some("Search for results".to_string()),
        parameters_schema: schema,
    }
}

#[test]
fn compiles_valid_object_schema() {
    let definition = test_definition(json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" }
        },
        "required": ["query"],
        "additionalProperties": false
    }));

    let compiled = CompiledToolSchema::from_definition(&definition);
    assert!(compiled.is_ok());
}

#[test]
fn fails_when_root_is_not_object_schema() {
    let definition = test_definition(json!("not-a-schema-object"));
    let error = CompiledToolSchema::from_definition(&definition).unwrap_err();
    assert!(matches!(error, ToolSchemaError::RootSchemaMustBeObject));
}

#[test]
fn fails_when_schema_is_invalid() {
    let definition = test_definition(json!({
        "type": "object",
        "properties": 12
    }));
    let error = CompiledToolSchema::from_definition(&definition).unwrap_err();
    assert!(matches!(error, ToolSchemaError::SchemaCompilation { .. }));
}

fn compiled_test_schema() -> CompiledToolSchema {
    let definition = test_definition(json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" },
            "count": { "type": "integer" }
        },
        "required": ["query"],
        "additionalProperties": false
    }));

    CompiledToolSchema::from_definition(&definition).expect("schema should compile")
}

#[test]
fn validate_args_rejects_non_object_values() {
    let schema = compiled_test_schema();
    let error = schema.validate_args(&json!(["query"])).unwrap_err();
    assert!(matches!(error, ToolArgsValidationError::ArgsMustBeObject));
}

#[test]
fn validate_args_rejects_missing_required_field() {
    let schema = compiled_test_schema();
    let error = schema.validate_args(&json!({})).unwrap_err();

    match error {
        ToolArgsValidationError::ValidationFailed { issues, .. } => {
            assert!(issues.iter().any(|issue| {
                issue.instance_path == "$" && issue.message.to_lowercase().contains("required")
            }));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn validate_args_rejects_type_mismatch() {
    let schema = compiled_test_schema();
    let error = schema
        .validate_args(&json!({"query": "rust", "count": "three"}))
        .unwrap_err();

    match error {
        ToolArgsValidationError::ValidationFailed { issues, .. } => {
            assert!(issues.iter().any(|issue| issue.instance_path == "/count"
                && issue.message.to_lowercase().contains("type")));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn validate_args_rejects_additional_properties() {
    let schema = compiled_test_schema();
    let error = schema
        .validate_args(&json!({"query": "rust", "extra": true}))
        .unwrap_err();

    match error {
        ToolArgsValidationError::ValidationFailed { issues, .. } => {
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.message.to_lowercase().contains("additional"))
            );
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn validate_args_accepts_valid_payload() {
    let schema = compiled_test_schema();
    let result = schema.validate_args(&json!({"query": "rust", "count": 3}));
    assert!(result.is_ok());
}
