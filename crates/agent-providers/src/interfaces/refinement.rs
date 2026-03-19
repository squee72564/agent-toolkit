use std::fmt::Debug;

use agent_core::{ProviderKind, ProviderOptions, Response, ResponseFormat, TaskRequest};
use serde_json::Value;

use crate::{
    error::{AdapterError, ProviderErrorInfo},
    interfaces::ProviderStreamProjector,
    providers::{
        anthropic::refinement::AnthropicRefinement, openai::refinement::OpenAiRefinement,
        openai_compatible::refinement::GenericOpenAiCompatibleRefinement,
        openrouter::refinement::OpenRouterRefinement,
    },
    request_plan::EncodedFamilyRequest,
};

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

static OPENAI_REFINEMENT: OpenAiRefinement = OpenAiRefinement;
static ANTHROPIC_REFINEMENT: AnthropicRefinement = AnthropicRefinement;
static OPENROUTER_REFINEMENT: OpenRouterRefinement = OpenRouterRefinement;
static GENERIC_OPENAI_COMPATIBLE_REFINEMENT: GenericOpenAiCompatibleRefinement =
    GenericOpenAiCompatibleRefinement;

pub(crate) fn refinement_for(kind: ProviderKind) -> &'static dyn ProviderRefinement {
    match kind {
        ProviderKind::OpenAi => &OPENAI_REFINEMENT,
        ProviderKind::Anthropic => &ANTHROPIC_REFINEMENT,
        ProviderKind::OpenRouter => &OPENROUTER_REFINEMENT,
        ProviderKind::GenericOpenAiCompatible => &GENERIC_OPENAI_COMPATIBLE_REFINEMENT,
    }
}
