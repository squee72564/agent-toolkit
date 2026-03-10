//! Tool registration, schema validation, and execution utilities.
//!
//! This crate provides a small tool runtime built around three core types:
//! [`ToolBuilder`] for constructing tools, [`ToolRegistry`] for collecting them,
//! and [`ToolRuntime`] for validating and executing calls against the registry.
//!
//! Typical usage:
//! 1. Build tools from raw JSON handlers or strongly typed handlers.
//! 2. Register the built tools in a [`ToolRegistry`].
//! 3. Execute validated tool calls through a [`ToolRuntime`].
//!
//! The runtime validates input arguments against each tool's JSON schema before
//! dispatch and reports schema, validation, and execution failures through
//! crate-specific error types.

mod builder;
mod registry;
mod runtime;
mod schema;
mod tool;

pub use builder::{ToolBuilder, ToolBuilderError};
pub use registry::{ToolRegistry, ToolRegistryError};
pub use runtime::{ToolRuntime, ToolRuntimeError};
pub use schema::{CompiledToolSchema, ToolArgsValidationError, ToolSchemaError, ValidationIssue};
pub use tool::{BuiltTool, Tool, ToolError, ToolOutput};
