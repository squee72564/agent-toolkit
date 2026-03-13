//! Provider-agnostic types for requests, responses, messages, tools, and platform metadata.

/// Provider identity and descriptor types shared across runtime and adapters.
pub mod identity;
/// Message roles and helper constructors for conversational inputs.
pub mod message;
/// Layered family-scoped and provider-scoped native request controls.
pub mod native_options;
/// Shared planning and execution-boundary types.
pub mod planning;
/// Provider and transport configuration shared with HTTP adapters.
pub mod platform;
/// Normalized response models returned from provider adapters.
pub mod response;
/// Task and legacy request models sent from the runtime to provider adapters.
pub mod task;
/// Tool definitions and mixed-content message parts.
pub mod tool;

/// Re-export of [`identity::ProviderCapabilities`].
pub use identity::ProviderCapabilities;
/// Re-export of [`identity::ProviderDescriptor`].
pub use identity::ProviderDescriptor;
/// Re-export of [`identity::ProviderFamilyId`].
pub use identity::ProviderFamilyId;
/// Re-export of [`identity::ProviderId`].
pub use identity::ProviderId;
/// Re-export of [`identity::ProviderInstanceId`].
pub use identity::ProviderInstanceId;
/// Re-export of [`identity::ProviderKind`].
pub use identity::ProviderKind;
/// Re-export of [`message::Message`].
pub use message::Message;
/// Re-export of [`message::MessageRole`].
pub use message::MessageRole;
/// Re-export of [`native_options::AnthropicFamilyOptions`].
pub use native_options::AnthropicFamilyOptions;
/// Re-export of [`native_options::AnthropicOptions`].
pub use native_options::AnthropicOptions;
/// Re-export of [`native_options::FamilyOptions`].
pub use native_options::FamilyOptions;
/// Re-export of [`native_options::NativeOptions`].
pub use native_options::NativeOptions;
/// Re-export of [`native_options::OpenAiCompatibleOptions`].
pub use native_options::OpenAiCompatibleOptions;
/// Re-export of [`native_options::OpenAiOptions`].
pub use native_options::OpenAiOptions;
/// Re-export of [`native_options::OpenRouterOptions`].
pub use native_options::OpenRouterOptions;
/// Re-export of [`native_options::ProviderOptions`].
pub use native_options::ProviderOptions;
/// Re-export of [`planning::ExecutionPlan`].
pub use planning::ExecutionPlan;
/// Re-export of [`planning::ResolvedAuthContext`].
pub use planning::ResolvedAuthContext;
/// Re-export of [`planning::ResolvedProviderAttempt`].
pub use planning::ResolvedProviderAttempt;
/// Re-export of [`planning::ResolvedTransportOptions`].
pub use planning::ResolvedTransportOptions;
/// Re-export of [`planning::ResponseMode`].
pub use planning::ResponseMode;
/// Re-export of [`planning::RetryPolicy`].
pub use planning::RetryPolicy;
/// Re-export of [`planning::TransportTimeoutOverrides`].
pub use planning::TransportTimeoutOverrides;
/// REFACTOR-SHIM: re-export of the legacy [`platform::AdapterContext`] surface.
pub use platform::AdapterContext;
/// Re-export of [`platform::AuthCredentials`].
pub use platform::AuthCredentials;
/// Re-export of [`platform::AuthStyle`].
pub use platform::AuthStyle;
/// Re-export of [`platform::PlatformConfig`].
pub use platform::PlatformConfig;
/// Re-export of [`platform::ProtocolKind`].
pub use platform::ProtocolKind;
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
/// Re-export of [`task::Request`].
pub use task::Request;
/// Re-export of [`task::ResponseFormat`].
pub use task::ResponseFormat;
/// Re-export of [`task::TaskRequest`].
pub use task::TaskRequest;
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
