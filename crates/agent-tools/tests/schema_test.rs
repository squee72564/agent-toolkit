use agent_core::types::ToolDefinition;
use agent_tools::{CompiledToolSchema, ToolArgsValidationError, ToolSchemaError, ValidationIssue};
use serde_json::json;
use std::cmp::Ordering;

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
fn compiles_valid_object_schema_when_root_type_is_union_with_object() {
    let definition = test_definition(json!({
        "type": ["null", "object"],
        "properties": {
            "query": { "type": "string" }
        }
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
fn fails_when_root_type_union_does_not_include_object() {
    let definition = test_definition(json!({
        "type": ["null", "string"]
    }));
    let error = CompiledToolSchema::from_definition(&definition).unwrap_err();
    assert!(matches!(error, ToolSchemaError::RootSchemaMustBeObject));
}

#[test]
fn compiles_root_object_schema_with_properties_but_no_type_declaration() {
    let definition = test_definition(json!({
        "properties": {
            "query": { "type": "string" }
        }
    }));

    let compiled = CompiledToolSchema::from_definition(&definition);
    assert!(compiled.is_ok());
}

#[test]
fn compiles_root_schema_with_ref_but_no_explicit_type() {
    let definition = test_definition(json!({
        "$ref": "#/$defs/query_tool",
        "$defs": {
            "query_tool": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }
        }
    }));

    let compiled = CompiledToolSchema::from_definition(&definition);
    assert!(compiled.is_ok());
}

#[test]
fn fails_when_root_type_is_not_string_or_array() {
    let definition = test_definition(json!({
        "type": 7
    }));

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
    let (_message, issues) =
        unwrap_validation_failed(schema.validate_args(&json!({})).unwrap_err());

    assert!(issues.iter().any(|issue| {
        issue.instance_path == "$"
            && issue.keyword_path.contains("required")
            && issue.message.to_lowercase().contains("required")
    }));
}

#[test]
fn validate_args_rejects_type_mismatch() {
    let schema = compiled_test_schema();
    let (_message, issues) = unwrap_validation_failed(
        schema
            .validate_args(&json!({"query": "rust", "count": "three"}))
            .unwrap_err(),
    );

    assert!(issues.iter().any(|issue| {
        issue.instance_path == "/count" && issue.message.to_lowercase().contains("type")
    }));
}

#[test]
fn validate_args_rejects_additional_properties() {
    let schema = compiled_test_schema();
    let (_message, issues) = unwrap_validation_failed(
        schema
            .validate_args(&json!({"query": "rust", "extra": true}))
            .unwrap_err(),
    );

    assert!(
        issues
            .iter()
            .any(|issue| issue.message.to_lowercase().contains("additional"))
    );
}

#[test]
fn validate_args_accepts_valid_payload() {
    let schema = compiled_test_schema();
    let result = schema.validate_args(&json!({"query": "rust", "count": 3}));
    assert!(result.is_ok());
}

#[test]
fn validate_args_reports_sorted_issues_and_stable_message() {
    let schema = compiled_test_schema();
    let (message, issues) = unwrap_validation_failed(
        schema
            .validate_args(&json!({
                "count": "three",
                "extra": true
            }))
            .unwrap_err(),
    );

    assert!(issues.len() >= 2);
    for window in issues.windows(2) {
        let left = &window[0];
        let right = &window[1];
        let ordering = left
            .instance_path
            .cmp(&right.instance_path)
            .then(left.keyword_path.cmp(&right.keyword_path))
            .then(left.message.cmp(&right.message));
        assert!(ordering == Ordering::Less || ordering == Ordering::Equal);
    }

    let reconstructed = issues
        .iter()
        .map(|issue| {
            format!(
                "{} [{}]: {}",
                issue.instance_path, issue.keyword_path, issue.message
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    assert_eq!(message, reconstructed);
    assert!(!message.is_empty());
}

#[test]
fn validate_args_uses_normalized_instance_path_for_root_errors() {
    let schema = compiled_test_schema();
    let (_message, issues) =
        unwrap_validation_failed(schema.validate_args(&json!({})).unwrap_err());

    assert!(issues.iter().all(|issue| !issue.instance_path.is_empty()));
    assert!(issues.iter().any(|issue| issue.instance_path == "$"));
}

fn unwrap_validation_failed(error: ToolArgsValidationError) -> (String, Vec<ValidationIssue>) {
    match error {
        ToolArgsValidationError::ValidationFailed { message, issues } => (message, issues),
        other => panic!("expected ValidationFailed, got: {other}"),
    }
}
