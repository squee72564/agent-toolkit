use agent_core::{
    FamilyOptions, OpenAiCompatibleOptions, ProviderKind, Response, ResponseFormat, ResponseMode,
    TaskRequest,
};
use agent_transport::HttpRequestOptions;
use reqwest::{Method, header::HeaderMap};
use serde_json::Value;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo};
use crate::family_codec::ProviderFamilyCodec;
use crate::openai_family::decode::{
    decode_openai_error, decode_openai_response, parse_openai_error_value,
};
use crate::openai_family::encode::encode_openai_request;
use crate::openai_family::{OpenAiDecodeEnvelope, OpenAiFamilyError, OpenAiFamilyErrorKind};
use crate::request_plan::{EncodedFamilyRequest, TransportResponseFraming};
use crate::stream_projector::ProviderStreamProjector;

use super::openai_compatible_stream_projector::OpenAiStreamProjector;

#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenAiCompatibleFamilyCodec;

impl ProviderFamilyCodec for OpenAiCompatibleFamilyCodec {
    fn encode_task(
        &self,
        task: &TaskRequest,
        model: &str,
        response_mode: ResponseMode,
        family_options: Option<&FamilyOptions>,
    ) -> Result<EncodedFamilyRequest, AdapterError> {
        let family_options = parse_family_options(family_options)?;
        let encoded = encode_openai_request(task, model).map_err(map_openai_plan_error)?;
        let mut body = encoded.body;
        if response_mode == ResponseMode::Streaming {
            body["stream"] = Value::Bool(true);
        }

        apply_family_options(&mut body, family_options)?;

        Ok(EncodedFamilyRequest {
            body,
            warnings: encoded.warnings,
            method: Method::POST,
            response_framing: if response_mode == ResponseMode::Streaming {
                TransportResponseFraming::Sse
            } else {
                TransportResponseFraming::Json
            },
            endpoint_path_override: None,
            provider_headers: HeaderMap::new(),
            request_options: if response_mode == ResponseMode::Streaming {
                HttpRequestOptions::sse_defaults()
            } else {
                HttpRequestOptions::json_defaults().with_allow_error_status(true)
            },
        })
    }

    fn decode_response(
        &self,
        body: Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        decode_openai_response(&OpenAiDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        })
        .map_err(|error| refine_family_decode_error(&body, error))
    }

    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        decode_openai_error(body)
    }

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        Box::<OpenAiStreamProjector>::default()
    }
}

fn parse_family_options(
    family_options: Option<&FamilyOptions>,
) -> Result<Option<&OpenAiCompatibleOptions>, AdapterError> {
    match family_options {
        Some(FamilyOptions::OpenAiCompatible(options)) => Ok(Some(options)),
        Some(FamilyOptions::Anthropic(_)) => Err(AdapterError::new(
            AdapterErrorKind::Validation,
            ProviderKind::OpenAi,
            AdapterOperation::PlanRequest,
            "OpenAI-compatible codec received mismatched family native options",
        )),
        None => Ok(None),
    }
}

fn apply_family_options(
    body: &mut Value,
    family_options: Option<&OpenAiCompatibleOptions>,
) -> Result<(), AdapterError> {
    let Some(body) = body.as_object_mut() else {
        return Err(AdapterError::new(
            AdapterErrorKind::ProtocolViolation,
            ProviderKind::OpenAi,
            AdapterOperation::PlanRequest,
            "OpenAI family request body must be an object",
        ));
    };

    if let Some(options) = family_options {
        if let Some(parallel_tool_calls) = options.parallel_tool_calls {
            body.insert(
                "parallel_tool_calls".to_string(),
                Value::Bool(parallel_tool_calls),
            );
        }
        if let Some(reasoning) = options.reasoning.as_ref() {
            body.insert("reasoning".to_string(), reasoning.clone());
        }
    }

    Ok(())
}

fn map_openai_plan_error(error: OpenAiFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_openai_family_error_kind(error.kind()),
        ProviderKind::OpenAi,
        AdapterOperation::PlanRequest,
        message,
        error,
    )
}

fn refine_family_decode_error(body: &Value, error: OpenAiFamilyError) -> AdapterError {
    let message = error.message().to_string();
    let error = AdapterError::with_source(
        map_openai_family_error_kind(error.kind()),
        ProviderKind::OpenAi,
        AdapterOperation::DecodeResponse,
        message,
        error,
    );
    refine_openai_compatible_error_metadata(body, error)
}

fn map_openai_family_error_kind(kind: OpenAiFamilyErrorKind) -> AdapterErrorKind {
    match kind {
        OpenAiFamilyErrorKind::Validation => AdapterErrorKind::Validation,
        OpenAiFamilyErrorKind::Encode => AdapterErrorKind::Encode,
        OpenAiFamilyErrorKind::Decode => AdapterErrorKind::Decode,
        OpenAiFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
        OpenAiFamilyErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        OpenAiFamilyErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}

pub(crate) fn refine_openai_compatible_error_metadata(
    body: &Value,
    mut error: AdapterError,
) -> AdapterError {
    if let Some(envelope) = parse_openai_error_value(body)
        && let Some(provider_code) = envelope.code.as_deref().or(envelope.error_type.as_deref())
    {
        error = error.with_provider_code(provider_code);
    }

    error
}
