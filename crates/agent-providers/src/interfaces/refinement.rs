use std::fmt::Debug;

use agent_core::{ProviderKind, ProviderOptions, Response, ResponseFormat, TaskRequest};
use serde_json::Value;

use crate::error::{AdapterError, ProviderErrorInfo};
use crate::interfaces::ProviderStreamProjector;
use crate::refinement::{
    AnthropicOverlay, GenericOpenAiCompatibleOverlay, OpenAiOverlay, OpenRouterOverlay,
};
use crate::request_plan::EncodedFamilyRequest;

/// Provider-specific refinement layer applied on top of a family codec.
///
/// Refinements own provider-specific request mutations, error refinement, and
/// optional response or stream overrides without changing the base family codec.
pub(crate) trait ProviderRefinement: Debug + Sync {
    fn refine_request(
        &self,
        task: &TaskRequest,
        model: &str,
        request: &mut EncodedFamilyRequest,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<(), AdapterError>;

    fn decode_provider_error(&self, body: &Value) -> Option<ProviderErrorInfo>;

    fn decode_response_override(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Option<Result<Response, AdapterError>>;

    fn create_stream_projector_override(&self) -> Option<Box<dyn ProviderStreamProjector>>;
}

static OPENAI_OVERLAY: OpenAiOverlay = OpenAiOverlay;
static ANTHROPIC_OVERLAY: AnthropicOverlay = AnthropicOverlay;
static OPENROUTER_OVERLAY: OpenRouterOverlay = OpenRouterOverlay;
static GENERIC_OPENAI_COMPATIBLE_OVERLAY: GenericOpenAiCompatibleOverlay =
    GenericOpenAiCompatibleOverlay;

pub(crate) fn refinement_for(kind: ProviderKind) -> &'static dyn ProviderRefinement {
    match kind {
        ProviderKind::OpenAi => &OPENAI_OVERLAY,
        ProviderKind::Anthropic => &ANTHROPIC_OVERLAY,
        ProviderKind::OpenRouter => &OPENROUTER_OVERLAY,
        ProviderKind::GenericOpenAiCompatible => &GENERIC_OPENAI_COMPATIBLE_OVERLAY,
    }
}
