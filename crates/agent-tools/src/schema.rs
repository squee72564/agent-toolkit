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

        if !permits_object_root_schema(schema_object) {
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
            Err(errors) => errors.map(to_validation_issue).collect(),
        };

        issues.sort_by(|left, right| {
            left.instance_path
                .cmp(&right.instance_path)
                .then(left.keyword_path.cmp(&right.keyword_path))
                .then(left.message.cmp(&right.message))
        });

        let message = build_validation_failed_message(&issues);

        Err(ToolArgsValidationError::ValidationFailed { message, issues })
    }
}

impl ToolArgsValidationError {
    pub(crate) fn decode_failure(name: &str, message: String) -> Self {
        let issue = ValidationIssue {
            instance_path: "$".to_string(),
            keyword_path: "$".to_string(),
            message: message.clone(),
        };

        Self::ValidationFailed {
            message: format!("tool '{name}' input decode failed: {message}"),
            issues: vec![issue],
        }
    }
}

fn permits_object_root_schema(schema_object: &Map<String, Value>) -> bool {
    match schema_object.get("type") {
        Some(type_value) => declares_object_type_value(type_value),
        None => schema_object.keys().any(|key| {
            matches!(
                key.as_str(),
                "$ref"
                    | "additionalProperties"
                    | "allOf"
                    | "anyOf"
                    | "dependentRequired"
                    | "dependentSchemas"
                    | "else"
                    | "if"
                    | "maxProperties"
                    | "minProperties"
                    | "not"
                    | "oneOf"
                    | "patternProperties"
                    | "properties"
                    | "propertyNames"
                    | "required"
                    | "then"
                    | "unevaluatedProperties"
            )
        }),
    }
}

fn declares_object_type_value(type_value: &Value) -> bool {
    match type_value {
        Value::String(value) => value == "object",
        Value::Array(values) => values
            .iter()
            .any(|value| matches!(value, Value::String(item) if item == "object")),
        _ => false,
    }
}

fn to_validation_issue(error: jsonschema::ValidationError<'_>) -> ValidationIssue {
    ValidationIssue {
        instance_path: normalize_json_pointer(&error.instance_path.to_string()),
        keyword_path: normalize_json_pointer(&error.schema_path.to_string()),
        message: error.to_string(),
    }
}

fn build_validation_failed_message(issues: &[ValidationIssue]) -> String {
    if issues.is_empty() {
        return "tool arguments failed schema validation".to_string();
    }

    issues
        .iter()
        .map(|issue| {
            format!(
                "{} [{}]: {}",
                issue.instance_path, issue.keyword_path, issue.message
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn normalize_json_pointer(path: &str) -> String {
    match path {
        "" | "#" => "$".to_string(),
        _ => path
            .strip_prefix('#')
            .map_or_else(|| path.to_string(), |stripped| format!("${stripped}")),
    }
}
