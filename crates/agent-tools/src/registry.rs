use std::collections::HashMap;

use agent_core::types::ToolDefinition;
use thiserror::Error;

use crate::schema::{CompiledToolSchema, ToolSchemaError};
use crate::tool::Tool;

#[derive(Debug, Error)]
pub enum ToolRegistryError {
    #[error("tool schema for '{name}' is invalid: {source}")]
    InvalidSchema {
        name: String,
        #[source]
        source: ToolSchemaError,
    },
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
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
    }

    pub fn register_validated<T>(&mut self, tool: T) -> Result<(), ToolRegistryError>
    where
        T: Tool + 'static,
    {
        let name = tool.name().to_string();
        let definition = tool_definition_from_tool(&tool);
        CompiledToolSchema::from_definition(&definition).map_err(|source| {
            ToolRegistryError::InvalidSchema {
                name: name.clone(),
                source,
            }
        })?;

        self.tools.insert(name, Box::new(tool));
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
}

fn tool_definition_from_tool(tool: &dyn Tool) -> ToolDefinition {
    ToolDefinition {
        name: tool.name().to_string(),
        description: tool.description().map(ToString::to_string),
        parameters_schema: tool.input_schema(),
    }
}
