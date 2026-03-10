//! Core tool traits and runtime value types.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

pub(crate) type BoxToolFuture = Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send>>;
pub(crate) type ToolHandler = dyn Fn(Value) -> BoxToolFuture + Send + Sync;

/// JSON output returned by a tool execution.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    /// Arbitrary JSON payload produced by the tool.
    pub content: Value,
}

#[derive(Debug, Error)]
pub enum ToolError {
    /// The handler failed while performing tool logic.
    #[error("tool execution failed: {0}")]
    Execution(String),
    /// Decoding raw JSON into typed input failed.
    #[error("tool input decode failed: {0}")]
    InvalidInputDecode(String),
    /// Encoding typed output into JSON failed.
    #[error("tool output encode failed: {0}")]
    InvalidOutputEncode(String),
}

/// Concrete tool implementation produced by [`crate::ToolBuilder`].
pub struct BuiltTool {
    name: String,
    description: Option<String>,
    schema: Value,
    pub(crate) handler: Arc<ToolHandler>,
}

#[async_trait]
/// Common interface implemented by executable tools.
pub trait Tool: Send + Sync {
    /// Returns the unique tool name used for registration and dispatch.
    fn name(&self) -> &str;
    /// Returns an optional human-readable description of the tool.
    fn description(&self) -> Option<&str>;
    /// Returns the JSON schema used to validate input arguments.
    fn input_schema(&self) -> Value;

    /// Executes the tool with raw JSON arguments.
    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError>;
}

impl BuiltTool {
    pub(crate) fn new(
        name: String,
        description: Option<String>,
        schema: Value,
        handler: Arc<ToolHandler>,
    ) -> Self {
        Self {
            name,
            description,
            schema,
            handler,
        }
    }
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
