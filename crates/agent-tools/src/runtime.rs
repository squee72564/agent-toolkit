use agent_core::types::ToolDefinition;
use serde_json::Value;
use thiserror::Error;

use crate::registry::ToolRegistry;
use crate::schema::{CompiledToolSchema, ToolArgsValidationError, ToolSchemaError};
use crate::tool::{Tool, ToolError, ToolOutput};

#[derive(Debug, Error)]
pub enum ToolRuntimeError {
    #[error("unknown tool: {name}")]
    UnknownTool { name: String },
    #[error("tool schema for '{name}' is invalid: {source}")]
    InvalidSchema {
        name: String,
        #[source]
        source: ToolSchemaError,
    },
    #[error("tool arguments for '{name}' are invalid: {source}")]
    InvalidArgs {
        name: String,
        #[source]
        source: ToolArgsValidationError,
    },
    #[error("tool '{name}' execution failed: {source}")]
    Execution {
        name: String,
        #[source]
        source: ToolError,
    },
}

pub struct ToolRuntime<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> ToolRuntime<'a> {
    pub fn new(registry: &'a ToolRegistry) -> Self {
        Self { registry }
    }

    pub fn validate_call(&self, name: &str, args: &Value) -> Result<(), ToolRuntimeError> {
        let tool = self.lookup_tool(name)?;
        let definition = tool_definition_from_tool(tool);
        let compiled_schema = CompiledToolSchema::from_definition(&definition)
            .map_err(|source| Self::invalid_schema(name, source))?;

        compiled_schema
            .validate_args(args)
            .map_err(|source| Self::invalid_args(name, source))
    }

    pub async fn execute(&self, name: &str, args: Value) -> Result<ToolOutput, ToolRuntimeError> {
        self.validate_call(name, &args)?;

        let tool = self.lookup_tool(name)?;

        match tool.execute(args).await {
            Ok(output) => Ok(output),
            Err(ToolError::InvalidInputDecode(message)) => Err(Self::invalid_args(
                name,
                ToolArgsValidationError::ValidationFailed {
                    message: format!("tool '{name}' input decode failed: {message}"),
                    issues: Vec::new(),
                },
            )),
            Err(source) => Err(Self::execution(name, source)),
        }
    }

    fn lookup_tool(&self, name: &str) -> Result<&dyn Tool, ToolRuntimeError> {
        self.registry
            .get(name)
            .ok_or_else(|| Self::unknown_tool(name))
    }

    fn unknown_tool(name: &str) -> ToolRuntimeError {
        ToolRuntimeError::UnknownTool {
            name: name.to_string(),
        }
    }

    fn invalid_schema(name: &str, source: ToolSchemaError) -> ToolRuntimeError {
        ToolRuntimeError::InvalidSchema {
            name: name.to_string(),
            source,
        }
    }

    fn invalid_args(name: &str, source: ToolArgsValidationError) -> ToolRuntimeError {
        ToolRuntimeError::InvalidArgs {
            name: name.to_string(),
            source,
        }
    }

    fn execution(name: &str, source: ToolError) -> ToolRuntimeError {
        ToolRuntimeError::Execution {
            name: name.to_string(),
            source,
        }
    }
}

fn tool_definition_from_tool(tool: &dyn Tool) -> ToolDefinition {
    ToolDefinition {
        name: tool.name().to_string(),
        description: tool.description().map(ToString::to_string),
        parameters_schema: tool.input_schema(),
    }
}
