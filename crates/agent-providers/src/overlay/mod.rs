use std::fmt::Debug;

use agent_core::{ProviderKind, ProviderOptions, Response, ResponseFormat, TaskRequest};
use serde_json::Value;

use crate::error::{AdapterError, ProviderErrorInfo};
use crate::request_plan::EncodedFamilyRequest;
use crate::streaming::ProviderStreamProjector;

mod anthropic;
mod generic_openai_compatible;
mod openai;
mod openrouter;
mod openrouter_stream_projector;

#[cfg(test)]
pub(crate) use openrouter::OpenRouterOverrides;

use anthropic::AnthropicOverlay;
use generic_openai_compatible::GenericOpenAiCompatibleOverlay;
use openai::OpenAiOverlay;
use openrouter::OpenRouterOverlay;

pub(crate) trait ProviderOverlay: Debug + Sync {
    fn apply_provider_overlay(
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

pub(crate) fn overlay_for(kind: ProviderKind) -> &'static dyn ProviderOverlay {
    match kind {
        ProviderKind::OpenAi => &OPENAI_OVERLAY,
        ProviderKind::Anthropic => &ANTHROPIC_OVERLAY,
        ProviderKind::OpenRouter => &OPENROUTER_OVERLAY,
        ProviderKind::GenericOpenAiCompatible => &GENERIC_OPENAI_COMPATIBLE_OVERLAY,
    }
}

#[cfg(test)]
mod anthropic_overlay_test;
#[cfg(test)]
mod anthropic_request_test;
#[cfg(test)]
mod anthropic_response_test;
#[cfg(test)]
mod openai_overlay_test;
#[cfg(test)]
mod openai_request_test;
#[cfg(test)]
mod openai_response_test;
#[cfg(test)]
mod openrouter_decoded_fixtures_test;
#[cfg(test)]
mod openrouter_overlay_test;
#[cfg(test)]
mod openrouter_request_test;
#[cfg(test)]
mod openrouter_response_test;
#[cfg(test)]
mod openrouter_stream_test;
