//! Builder utilities for defining tools from schemas and handlers.

use std::sync::Arc;

use agent_core::types::ToolDefinition;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use crate::schema::{CompiledToolSchema, ToolSchemaError};
use crate::tool::{BoxToolFuture, BuiltTool, ToolError, ToolHandler, ToolOutput};

#[derive(Debug, Error)]
pub enum ToolBuilderError {
    /// The builder did not receive a non-empty tool name.
    #[error("tool name is required")]
    MissingName,
    /// The builder does not have an input schema available.
    #[error("tool schema is required")]
    MissingSchema,
    /// The builder does not have an execution handler.
    #[error("tool handler is required")]
    MissingHandler,
    /// Converting a derived schema into JSON failed.
    #[error("tool schema could not be generated: {source}")]
    GeneratedSchema {
        #[source]
        source: serde_json::Error,
    },
    /// The supplied or derived schema failed validation.
    #[error("tool schema is invalid: {source}")]
    InvalidSchema {
        #[source]
        source: ToolSchemaError,
    },
}

/// Incrementally constructs a [`BuiltTool`] from metadata, schema, and handler state.
#[derive(Default)]
pub struct ToolBuilder {
    name: Option<String>,
    description: Option<String>,
    schema: Option<Value>,
    generated_schema_error: Option<ToolBuilderError>,
    handler: Option<Arc<ToolHandler>>,
}

impl ToolBuilder {
    /// Creates an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds the builder from an existing [`ToolDefinition`].
    ///
    /// This copies the tool metadata and schema but leaves the handler unset.
    pub fn from_definition(definition: ToolDefinition) -> Self {
        Self {
            name: Some(definition.name),
            description: definition.description,
            schema: Some(definition.parameters_schema),
            generated_schema_error: None,
            handler: None,
        }
    }

    /// Sets the tool name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the human-readable tool description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the JSON schema used to validate tool inputs.
    pub fn schema(mut self, schema: Value) -> Self {
        self.schema = Some(schema);
        self.generated_schema_error = None;
        self
    }

    /// Registers a handler that receives raw JSON arguments.
    pub fn handler<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolOutput, ToolError>> + Send + 'static,
    {
        let wrapped = move |args: Value| -> BoxToolFuture { Box::pin(handler(args)) };
        self.handler = Some(Arc::new(wrapped));
        self
    }

    /// Registers a strongly typed handler and derives an input JSON schema from `TArgs`.
    ///
    /// Behavior:
    /// - Decodes runtime JSON arguments into `TArgs`.
    /// - Executes `handler(TArgs)`.
    /// - Encodes `TOut` into `ToolOutput.content`.
    ///
    /// Conversion failures are mapped to:
    /// - `ToolError::InvalidInputDecode` when decoding `TArgs` fails.
    /// - `ToolError::InvalidOutputEncode` when encoding `TOut` fails.
    ///
    /// Schema precedence:
    /// - Calling `typed_handler` sets the schema from `TArgs` by default.
    /// - Calling `.schema(...)` after `typed_handler` overrides the derived schema.
    pub fn typed_handler<TArgs, TOut, F, Fut>(mut self, handler: F) -> Self
    where
        TArgs: DeserializeOwned + JsonSchema + Send + 'static,
        TOut: Serialize + Send + 'static,
        F: Fn(TArgs) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<TOut, ToolError>> + Send + 'static,
    {
        match derived_schema::<TArgs>() {
            Ok(schema) => {
                self.schema = Some(schema);
                self.generated_schema_error = None;
            }
            Err(error) => {
                self.schema = None;
                self.generated_schema_error = Some(error);
            }
        }
        let handler = Arc::new(handler);

        let wrapped = move |args: Value| -> BoxToolFuture {
            let handler = Arc::clone(&handler);
            let typed_args = match serde_json::from_value::<TArgs>(args) {
                Ok(typed_args) => typed_args,
                Err(error) => {
                    return Box::pin(async move {
                        Err(ToolError::InvalidInputDecode(error.to_string()))
                    });
                }
            };

            Box::pin(async move {
                let output = handler(typed_args).await?;
                let content = serde_json::to_value(output)
                    .map_err(|error| ToolError::InvalidOutputEncode(error.to_string()))?;
                Ok(ToolOutput { content })
            })
        };

        self.handler = Some(Arc::new(wrapped));
        self
    }

    /// Finalizes the builder into a [`BuiltTool`].
    ///
    /// This verifies required fields are present and confirms that the final
    /// schema can be compiled for runtime validation.
    pub fn build(self) -> Result<BuiltTool, ToolBuilderError> {
        let name = self.name.ok_or(ToolBuilderError::MissingName)?;
        if name.trim().is_empty() {
            return Err(ToolBuilderError::MissingName);
        }
        if let Some(error) = self.generated_schema_error {
            return Err(error);
        }
        let schema = self.schema.ok_or(ToolBuilderError::MissingSchema)?;
        let handler = self.handler.ok_or(ToolBuilderError::MissingHandler)?;

        let definition = ToolDefinition {
            name: name.clone(),
            description: self.description.clone(),
            parameters_schema: schema.clone(),
        };
        CompiledToolSchema::from_definition(&definition)
            .map_err(|source| ToolBuilderError::InvalidSchema { source })?;

        Ok(BuiltTool::new(name, self.description, schema, handler))
    }
}

fn derived_schema<TArgs>() -> Result<Value, ToolBuilderError>
where
    TArgs: JsonSchema,
{
    serde_json::to_value(schemars::schema_for!(TArgs))
        .map_err(|source| ToolBuilderError::GeneratedSchema { source })
}
