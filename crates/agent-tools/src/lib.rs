use std::collections::HashMap;

use agent_core::types::ToolDefinition;
use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

mod builder;
mod schema;

pub use builder::{BuiltTool, ToolBuilder, ToolBuilderError};
pub use schema::{CompiledToolSchema, ToolArgsValidationError, ToolSchemaError, ValidationIssue};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub content: Value,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool execution failed: {0}")]
    Execution(String),
}

#[derive(Debug, Error)]
pub enum ToolRegistryError {
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

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> Option<&str>;
    fn input_schema(&self) -> Value;

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError>;
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    compiled_schemas: HashMap<String, CompiledToolSchema>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(&mut self, tool: T)
    where
        T: Tool + 'static,
    {
        let name = tool.name().to_string();
        self.tools.insert(name.clone(), Box::new(tool));
        self.compiled_schemas.remove(&name);
    }

    pub fn register_validated<T>(&mut self, tool: T) -> Result<(), ToolRegistryError>
    where
        T: Tool + 'static,
    {
        let name = tool.name().to_string();
        let definition = tool_definition_from_tool(&tool);
        let compiled = CompiledToolSchema::from_definition(&definition).map_err(|source| {
            ToolRegistryError::InvalidSchema {
                name: name.clone(),
                source,
            }
        })?;

        self.tools.insert(name.clone(), Box::new(tool));
        self.compiled_schemas.insert(name, compiled);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|tool| tool.as_ref())
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut definitions: Vec<_> = self
            .tools
            .values()
            .map(|tool| tool_definition_from_tool(tool.as_ref()))
            .collect();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions
    }

    pub fn validate_call(&self, name: &str, args: &Value) -> Result<(), ToolRegistryError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolRegistryError::UnknownTool {
                name: name.to_string(),
            })?;

        if let Some(compiled_schema) = self.compiled_schemas.get(name) {
            return compiled_schema.validate_args(args).map_err(|source| {
                ToolRegistryError::InvalidArgs {
                    name: name.to_string(),
                    source,
                }
            });
        }

        let definition = tool_definition_from_tool(tool.as_ref());
        let compiled_schema =
            CompiledToolSchema::from_definition(&definition).map_err(|source| {
                ToolRegistryError::InvalidSchema {
                    name: name.to_string(),
                    source,
                }
            })?;

        compiled_schema
            .validate_args(args)
            .map_err(|source| ToolRegistryError::InvalidArgs {
                name: name.to_string(),
                source,
            })
    }

    pub async fn execute_validated(
        &self,
        name: &str,
        args: Value,
    ) -> Result<ToolOutput, ToolRegistryError> {
        self.validate_call(name, &args)?;

        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolRegistryError::UnknownTool {
                name: name.to_string(),
            })?;

        tool.execute(args)
            .await
            .map_err(|source| ToolRegistryError::Execution {
                name: name.to_string(),
                source,
            })
    }
}

fn tool_definition_from_tool(tool: &dyn Tool) -> ToolDefinition {
    ToolDefinition {
        name: tool.name().to_string(),
        description: tool.description().map(ToString::to_string),
        parameters_schema: tool.input_schema(),
    }
}
