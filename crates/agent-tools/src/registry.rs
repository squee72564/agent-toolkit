//! Storage and lookup for named tools.

use std::collections::HashMap;

use agent_core::types::ToolDefinition;
use thiserror::Error;

use crate::schema::{CompiledToolSchema, ToolSchemaError};
use crate::tool::Tool;

#[derive(Debug, Error)]
pub enum ToolRegistryError {
    /// A tool with the same name is already registered.
    #[error("tool '{name}' is already registered")]
    DuplicateName { name: String },
    /// A tool's declared input schema could not be compiled.
    #[error("tool schema for '{name}' is invalid: {source}")]
    InvalidSchema {
        name: String,
        #[source]
        source: ToolSchemaError,
    },
}

pub(crate) struct RegisteredTool {
    tool: Box<dyn Tool>,
    definition: ToolDefinition,
    compiled_schema: CompiledToolSchema,
}

impl RegisteredTool {
    pub(crate) fn tool(&self) -> &dyn Tool {
        self.tool.as_ref()
    }

    pub(crate) fn compiled_schema(&self) -> &CompiledToolSchema {
        &self.compiled_schema
    }
}

/// Owns a set of tools indexed by unique tool name.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
}

impl ToolRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a tool and validates its input schema.
    pub fn register<T>(&mut self, tool: T) -> Result<(), ToolRegistryError>
    where
        T: Tool + 'static,
    {
        self.insert(tool)
    }

    /// Registers a tool and validates its input schema.
    ///
    /// This currently behaves the same as [`Self::register`].
    pub fn register_validated<T>(&mut self, tool: T) -> Result<(), ToolRegistryError>
    where
        T: Tool + 'static,
    {
        self.insert(tool)
    }

    /// Returns the registered tool with the given name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(RegisteredTool::tool)
    }

    /// Returns the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns `true` when the registry has no tools.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Returns tool definitions sorted by tool name.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut definitions: Vec<_> = self
            .tools
            .values()
            .map(|entry| ToolDefinition {
                name: entry.definition.name.clone(),
                description: entry.definition.description.clone(),
                parameters_schema: entry.definition.parameters_schema.clone(),
            })
            .collect();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions
    }

    pub(crate) fn get_registered(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.get(name)
    }

    fn insert<T>(&mut self, tool: T) -> Result<(), ToolRegistryError>
    where
        T: Tool + 'static,
    {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            return Err(ToolRegistryError::DuplicateName { name });
        }

        let definition = tool_definition_from_tool(&tool);
        let compiled_schema =
            CompiledToolSchema::from_definition(&definition).map_err(|source| {
                ToolRegistryError::InvalidSchema {
                    name: name.clone(),
                    source,
                }
            })?;

        self.tools.insert(
            name,
            RegisteredTool {
                tool: Box::new(tool),
                definition,
                compiled_schema,
            },
        );
        Ok(())
    }
}

pub(crate) fn tool_definition_from_tool(tool: &dyn Tool) -> ToolDefinition {
    ToolDefinition {
        name: tool.name().to_string(),
        description: tool.description().map(ToString::to_string),
        parameters_schema: tool.input_schema(),
    }
}
