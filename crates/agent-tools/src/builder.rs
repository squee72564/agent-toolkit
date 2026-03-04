use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use agent_core::types::ToolDefinition;
use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

use crate::{CompiledToolSchema, Tool, ToolError, ToolOutput, ToolSchemaError};

type BoxToolFuture = Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send>>;
type ToolHandler = dyn Fn(Value) -> BoxToolFuture + Send + Sync;

pub struct BuiltTool {
    name: String,
    description: Option<String>,
    schema: Value,
    handler: Arc<ToolHandler>,
}

#[async_trait]
impl Tool for BuiltTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn input_schema(&self) -> Value {
        self.schema.clone()
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        (self.handler)(args).await
    }
}

#[derive(Debug, Error)]
pub enum ToolBuilderError {
    #[error("tool name is required")]
    MissingName,
    #[error("tool schema is required")]
    MissingSchema,
    #[error("tool handler is required")]
    MissingHandler,
    #[error("tool schema is invalid: {source}")]
    InvalidSchema {
        #[source]
        source: ToolSchemaError,
    },
}

#[derive(Default)]
pub struct ToolBuilder {
    name: Option<String>,
    description: Option<String>,
    schema: Option<Value>,
    handler: Option<Arc<ToolHandler>>,
}

impl ToolBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_definition(definition: ToolDefinition) -> Self {
        Self {
            name: Some(definition.name),
            description: definition.description,
            schema: Some(definition.parameters_schema),
            handler: None,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn schema(mut self, schema: Value) -> Self {
        self.schema = Some(schema);
        self
    }

    pub fn handler<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolOutput, ToolError>> + Send + 'static,
    {
        let wrapped = move |args: Value| -> BoxToolFuture { Box::pin(handler(args)) };
        self.handler = Some(Arc::new(wrapped));
        self
    }

    pub fn build(self) -> Result<BuiltTool, ToolBuilderError> {
        let name = self.name.ok_or(ToolBuilderError::MissingName)?;
        let schema = self.schema.ok_or(ToolBuilderError::MissingSchema)?;
        let handler = self.handler.ok_or(ToolBuilderError::MissingHandler)?;

        let definition = ToolDefinition {
            name: name.clone(),
            description: self.description.clone(),
            parameters_schema: schema.clone(),
        };
        CompiledToolSchema::from_definition(&definition)
            .map_err(|source| ToolBuilderError::InvalidSchema { source })?;

        Ok(BuiltTool {
            name,
            description: self.description,
            schema,
            handler,
        })
    }
}
