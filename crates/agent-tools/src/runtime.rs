use serde_json::Value;
use thiserror::Error;

use crate::registry::{RegisteredTool, ToolRegistry};
use crate::schema::{ToolArgsValidationError, ToolSchemaError};
use crate::tool::{ToolError, ToolOutput};

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
        let entry = self.lookup_tool(name)?;
        entry
            .compiled_schema()
            .validate_args(args)
            .map_err(|source| Self::invalid_args(name, source))
    }

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
