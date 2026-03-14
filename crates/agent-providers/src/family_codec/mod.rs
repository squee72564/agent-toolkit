use std::fmt::Debug;

use agent_core::{
    FamilyOptions, ProviderFamilyId, Response, ResponseFormat, ResponseMode, TaskRequest,
};
use serde_json::Value;

use crate::error::{AdapterError, ProviderErrorInfo};
use crate::request_plan::EncodedFamilyRequest;
use crate::streaming::ProviderStreamProjector;

mod anthropic;
mod anthropic_stream_projector;
mod openai_compatible;
mod openai_compatible_stream_projector;

pub(crate) use anthropic::AnthropicFamilyCodec;
pub(crate) use openai_compatible::OpenAiCompatibleFamilyCodec;

pub(crate) trait ProviderFamilyCodec: Debug + Sync {
    fn encode_task(
        &self,
        task: &TaskRequest,
        model: &str,
        response_mode: ResponseMode,
        family_options: Option<&FamilyOptions>,
    ) -> Result<EncodedFamilyRequest, AdapterError>;

    fn decode_response(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError>;

    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo>;

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector>;
}

static OPENAI_COMPATIBLE_CODEC: OpenAiCompatibleFamilyCodec = OpenAiCompatibleFamilyCodec;
static ANTHROPIC_CODEC: AnthropicFamilyCodec = AnthropicFamilyCodec;

pub(crate) fn codec_for(family: ProviderFamilyId) -> &'static dyn ProviderFamilyCodec {
    match family {
        ProviderFamilyId::OpenAiCompatible => &OPENAI_COMPATIBLE_CODEC,
        ProviderFamilyId::Anthropic => &ANTHROPIC_CODEC,
    }
}

#[cfg(test)]
mod anthropic_decoded_fixtures_test;
#[cfg(test)]
mod anthropic_request_test;
#[cfg(test)]
mod anthropic_response_test;
#[cfg(test)]
mod anthropic_stream_test;
#[cfg(test)]
mod openai_compatible_decoded_fixtures_test;
#[cfg(test)]
mod openai_compatible_request_test;
#[cfg(test)]
mod openai_compatible_response_test;
#[cfg(test)]
mod openai_compatible_stream_test;
