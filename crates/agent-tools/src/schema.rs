use agent_core::types::ToolDefinition;
use jsonschema::JSONSchema;
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub instance_path: String,
    pub keyword_path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ToolSchemaError {
    #[error("tool schema root must be a JSON object with type 'object'")]
    RootSchemaMustBeObject,
    #[error("tool schema compilation failed: {message}")]
    SchemaCompilation { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ToolArgsValidationError {
    #[error("tool arguments must be a JSON object")]
    ArgsMustBeObject,
    #[error("{message}")]
    ValidationFailed {
        message: String,
        issues: Vec<ValidationIssue>,
    },
}

#[derive(Debug)]
pub struct CompiledToolSchema {
    validator: JSONSchema,
}

impl CompiledToolSchema {
    pub fn from_definition(def: &ToolDefinition) -> Result<Self, ToolSchemaError> {
        let schema_object = def
            .parameters_schema
            .as_object()
            .ok_or(ToolSchemaError::RootSchemaMustBeObject)?;

        if !declares_object_type(schema_object) {
            return Err(ToolSchemaError::RootSchemaMustBeObject);
        }

        let validator = JSONSchema::options()
            .compile(&def.parameters_schema)
            .map_err(|error| ToolSchemaError::SchemaCompilation {
                message: error.to_string(),
            })?;

        Ok(Self { validator })
    }

    pub fn validate_args(&self, args: &Value) -> Result<(), ToolArgsValidationError> {
        if !args.is_object() {
            return Err(ToolArgsValidationError::ArgsMustBeObject);
        }

        let mut issues: Vec<ValidationIssue> = match self.validator.validate(args) {
            Ok(()) => return Ok(()),
            Err(errors) => errors
                .map(|error| ValidationIssue {
                    instance_path: normalize_json_pointer(error.instance_path.to_string()),
                    keyword_path: normalize_json_pointer(error.schema_path.to_string()),
                    message: error.to_string(),
                })
                .collect(),
        };

        issues.sort_by(|left, right| {
            left.instance_path
                .cmp(&right.instance_path)
                .then(left.keyword_path.cmp(&right.keyword_path))
                .then(left.message.cmp(&right.message))
        });

        let message = issues
            .iter()
            .map(|issue| format!("{}: {}", issue.instance_path, issue.message))
            .collect::<Vec<_>>()
            .join("; ");

        Err(ToolArgsValidationError::ValidationFailed { message, issues })
    }
}

fn declares_object_type(schema_object: &Map<String, Value>) -> bool {
    match schema_object.get("type") {
        Some(Value::String(value)) => value == "object",
        Some(Value::Array(values)) => values
            .iter()
            .any(|value| matches!(value, Value::String(item) if item == "object")),
        _ => false,
    }
}

fn normalize_json_pointer(path: String) -> String {
    if path.is_empty() {
        "$".to_string()
    } else {
        path
    }
}
