use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

pub(crate) type BoxToolFuture = Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send>>;
pub(crate) type ToolHandler = dyn Fn(Value) -> BoxToolFuture + Send + Sync;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub content: Value,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("tool input decode failed: {0}")]
    InvalidInputDecode(String),
    #[error("tool output encode failed: {0}")]
    InvalidOutputEncode(String),
}

pub struct BuiltTool {
    name: String,
    description: Option<String>,
    schema: Value,
    pub(crate) handler: Arc<ToolHandler>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> Option<&str>;
    fn input_schema(&self) -> Value;

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
