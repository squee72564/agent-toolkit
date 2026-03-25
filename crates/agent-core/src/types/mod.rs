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
/// Re-export of [`identity::ProviderInstanceId`].
pub use identity::ProviderInstanceId;
/// Re-export of [`identity::ProviderKind`].
pub use identity::ProviderKind;
/// Re-export of [`message::Message`].
pub use message::Message;
/// Re-export of [`message::MessageRole`].
pub use message::MessageRole;
/// Re-export of [`native_options::AnthropicCacheControl`].
pub use native_options::AnthropicCacheControl;
/// Re-export of [`native_options::AnthropicCacheControlTTL`].
pub use native_options::AnthropicCacheControlTTL;
/// Re-export of [`native_options::AnthropicCacheControlType`].
pub use native_options::AnthropicCacheControlType;
/// Re-export of [`native_options::AnthropicFamilyOptions`].
pub use native_options::AnthropicFamilyOptions;
/// Re-export of [`native_options::AnthropicOptions`].
pub use native_options::AnthropicOptions;
/// Re-export of [`native_options::AnthropicOutputConfig`].
pub use native_options::AnthropicOutputConfig;
/// Re-export of [`native_options::AnthropicOutputEffort`].
pub use native_options::AnthropicOutputEffort;
/// Re-export of [`native_options::AnthropicOutputFormat`].
pub use native_options::AnthropicOutputFormat;
/// Re-export of [`native_options::AnthropicOutputFormatType`].
pub use native_options::AnthropicOutputFormatType;
/// Re-export of [`native_options::AnthropicServiceTier`].
pub use native_options::AnthropicServiceTier;
/// Re-export of [`native_options::AnthropicThinking`].
pub use native_options::AnthropicThinking;
/// Re-export of [`native_options::AnthropicThinkingBudget`].
pub use native_options::AnthropicThinkingBudget;
/// Re-export of [`native_options::AnthropicThinkingDisplay`].
pub use native_options::AnthropicThinkingDisplay;
/// Re-export of [`native_options::AnthropicToolChoiceOptions`].
pub use native_options::AnthropicToolChoiceOptions;
/// Re-export of [`native_options::FamilyOptions`].
pub use native_options::FamilyOptions;
/// Re-export of [`native_options::NativeOptions`].
pub use native_options::NativeOptions;
/// Re-export of [`native_options::OpenAiCompatibleOptions`].
pub use native_options::OpenAiCompatibleOptions;
/// Re-export of [`native_options::OpenAiCompatibleReasoning`].
pub use native_options::OpenAiCompatibleReasoning;
/// Re-export of [`native_options::OpenAiCompatibleReasoningEffort`].
pub use native_options::OpenAiCompatibleReasoningEffort;
/// Re-export of [`native_options::OpenAiCompatibleReasoningSummary`].
pub use native_options::OpenAiCompatibleReasoningSummary;
/// Re-export of [`native_options::OpenAiOptions`].
pub use native_options::OpenAiOptions;
/// Re-export of [`native_options::OpenAiPromptCacheRetention`].
pub use native_options::OpenAiPromptCacheRetention;
/// Re-export of [`native_options::OpenAiServiceTier`].
pub use native_options::OpenAiServiceTier;
/// Re-export of [`native_options::OpenAiTextOptions`].
pub use native_options::OpenAiTextOptions;
/// Re-export of [`native_options::OpenAiTextVerbosity`].
pub use native_options::OpenAiTextVerbosity;
/// Re-export of [`native_options::OpenAiTruncation`].
pub use native_options::OpenAiTruncation;
/// Re-export of [`native_options::OpenRouterAutoRouterPlugin`].
pub use native_options::OpenRouterAutoRouterPlugin;
/// Re-export of [`native_options::OpenRouterContextCompressionEngine`].
pub use native_options::OpenRouterContextCompressionEngine;
/// Re-export of [`native_options::OpenRouterContextCompressionPlugin`].
pub use native_options::OpenRouterContextCompressionPlugin;
/// Re-export of [`native_options::OpenRouterFileParserPdfEngine`].
pub use native_options::OpenRouterFileParserPdfEngine;
/// Re-export of [`native_options::OpenRouterFileParserPdfOptions`].
pub use native_options::OpenRouterFileParserPdfOptions;
/// Re-export of [`native_options::OpenRouterFileParserPlugin`].
pub use native_options::OpenRouterFileParserPlugin;
/// Re-export of [`native_options::OpenRouterImageConfig`].
pub use native_options::OpenRouterImageConfig;
/// Re-export of [`native_options::OpenRouterImageConfigValue`].
pub use native_options::OpenRouterImageConfigValue;
/// Re-export of [`native_options::OpenRouterModerationPlugin`].
pub use native_options::OpenRouterModerationPlugin;
/// Re-export of [`native_options::OpenRouterOptions`].
pub use native_options::OpenRouterOptions;
/// Re-export of [`native_options::OpenRouterPlugin`].
pub use native_options::OpenRouterPlugin;
/// Re-export of [`native_options::OpenRouterResponseHealingPlugin`].
pub use native_options::OpenRouterResponseHealingPlugin;
/// Re-export of [`native_options::OpenRouterTextOptions`].
pub use native_options::OpenRouterTextOptions;
/// Re-export of [`native_options::OpenRouterTextVerbosity`].
pub use native_options::OpenRouterTextVerbosity;
/// Re-export of [`native_options::OpenRouterTrace`].
pub use native_options::OpenRouterTrace;
/// Re-export of [`native_options::OpenRouterWebPlugin`].
pub use native_options::OpenRouterWebPlugin;
/// Re-export of [`native_options::OpenRouterWebPluginEngine`].
pub use native_options::OpenRouterWebPluginEngine;
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
