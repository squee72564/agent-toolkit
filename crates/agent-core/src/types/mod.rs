//! Provider-agnostic types for requests, responses, messages, tools, and platform metadata.

/// Message roles and helper constructors for conversational inputs.
pub mod message;
/// Provider and transport configuration shared with HTTP adapters.
pub mod platform;
/// Request models sent from the runtime to provider adapters.
pub mod request;
/// Normalized response models returned from provider adapters.
pub mod response;
/// Tool definitions and mixed-content message parts.
pub mod tool;

/// Re-export of [`message::Message`].
pub use message::Message;
/// Re-export of [`message::MessageRole`].
pub use message::MessageRole;
/// Re-export of [`platform::AdapterContext`].
pub use platform::AdapterContext;
/// Re-export of [`platform::AuthCredentials`].
pub use platform::AuthCredentials;
/// Re-export of [`platform::AuthStyle`].
pub use platform::AuthStyle;
/// Re-export of [`platform::PlatformConfig`].
pub use platform::PlatformConfig;
/// Re-export of [`platform::ProtocolKind`].
pub use platform::ProtocolKind;
/// Re-export of [`platform::ProviderId`].
pub use platform::ProviderId;
/// Re-export of [`request::Request`].
pub use request::Request;
/// Re-export of [`request::ResponseFormat`].
pub use request::ResponseFormat;
/// Re-export of [`response::AssistantOutput`].
pub use response::AssistantOutput;
/// Re-export of [`response::FinishReason`].
pub use response::FinishReason;
/// Re-export of [`response::Response`].
pub use response::Response;
/// Re-export of [`response::RuntimeWarning`].
pub use response::RuntimeWarning;
/// Re-export of [`response::Usage`].
pub use response::Usage;
/// Re-export of [`tool::ContentPart`].
pub use tool::ContentPart;
/// Re-export of [`tool::ToolCall`].
pub use tool::ToolCall;
/// Re-export of [`tool::ToolChoice`].
pub use tool::ToolChoice;
/// Re-export of [`tool::ToolDefinition`].
pub use tool::ToolDefinition;
/// Re-export of [`tool::ToolResult`].
pub use tool::ToolResult;
/// Re-export of [`tool::ToolResultContent`].
pub use tool::ToolResultContent;
