//! Runtime validation and execution for registered tools.

use serde_json::Value;
use thiserror::Error;

use crate::registry::{RegisteredTool, ToolRegistry};
use crate::schema::{ToolArgsValidationError, ToolSchemaError};
use crate::tool::{ToolError, ToolOutput};

#[derive(Debug, Error)]
pub enum ToolRuntimeError {
    /// The requested tool name is not registered.
    #[error("unknown tool: {name}")]
    UnknownTool { name: String },
    /// The registered schema could not be used for validation.
    #[error("tool schema for '{name}' is invalid: {source}")]
    InvalidSchema {
        name: String,
        #[source]
        source: ToolSchemaError,
    },
    /// The supplied arguments failed validation or typed input decoding.
    #[error("tool arguments for '{name}' are invalid: {source}")]
    InvalidArgs {
        name: String,
        #[source]
        source: ToolArgsValidationError,
    },
    /// The tool handler returned an execution failure.
    #[error("tool '{name}' execution failed: {source}")]
    Execution {
        name: String,
        #[source]
        source: ToolError,
    },
}

/// Executes calls against a [`ToolRegistry`] after validating their arguments.
pub struct ToolRuntime<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> ToolRuntime<'a> {
    /// Creates a runtime bound to the provided registry.
    pub fn new(registry: &'a ToolRegistry) -> Self {
        Self { registry }
    }

    /// Validates a proposed tool call without executing it.
    pub fn validate_call(&self, name: &str, args: &Value) -> Result<(), ToolRuntimeError> {
        let entry = self.lookup_tool(name)?;
        entry
            .compiled_schema()
            .validate_args(args)
            .map_err(|source| Self::invalid_args(name, source))
    }

    /// Validates and executes a tool call.
    ///
    /// Raw input decode failures from typed handlers are normalized into
    /// [`ToolRuntimeError::InvalidArgs`] so callers can treat them the same as
    /// schema validation failures.
    pub async fn execute(&self, name: &str, args: Value) -> Result<ToolOutput, ToolRuntimeError> {
        let entry = self.lookup_tool(name)?;
        entry
            .compiled_schema()
            .validate_args(&args)
            .map_err(|source| Self::invalid_args(name, source))?;

        match entry.tool().execute(args).await {
            Ok(output) => Ok(output),
            Err(ToolError::InvalidInputDecode(message)) => Err(Self::invalid_args(
                name,
                ToolArgsValidationError::decode_failure(name, message),
            )),
            Err(source) => Err(Self::execution(name, source)),
        }
    }

    fn lookup_tool(&self, name: &str) -> Result<&RegisteredTool, ToolRuntimeError> {
        self.registry
            .get_registered(name)
            .ok_or_else(|| Self::unknown_tool(name))
    }

    fn unknown_tool(name: &str) -> ToolRuntimeError {
        ToolRuntimeError::UnknownTool {
            name: name.to_string(),
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
