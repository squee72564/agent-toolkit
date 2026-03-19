use std::fmt::Debug;

use agent_core::{
    FamilyOptions, ProviderFamilyId, Response, ResponseFormat, ResponseMode, TaskRequest,
};
use serde_json::Value;

use crate::error::{AdapterError, ProviderErrorInfo};
use crate::family_codec::{AnthropicFamilyCodec, OpenAiCompatibleFamilyCodec};
use crate::request_plan::EncodedFamilyRequest;
use crate::interfaces::ProviderStreamProjector;

/// Protocol-family translation boundary.
///
/// Family codecs encode semantic task requests into family-shaped transport
/// requests, decode family-shaped responses back into canonical `Response`
/// values, and provide the default family stream projector.
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
