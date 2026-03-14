use agent_core::{
    AnthropicFamilyOptions, FamilyOptions, ProviderKind, Response, ResponseFormat, ResponseMode,
    TaskRequest,
};
use agent_transport::HttpRequestOptions;
use reqwest::{Method, header::HeaderMap};
use serde_json::Value;

use crate::anthropic_family::decode::{
    decode_anthropic_error, decode_anthropic_response, parse_anthropic_error_value,
};
use crate::anthropic_family::encode::encode_anthropic_request;
use crate::anthropic_family::{
    AnthropicDecodeEnvelope, AnthropicFamilyError, AnthropicFamilyErrorKind,
};
use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo};
use crate::family_codec::ProviderFamilyCodec;
use crate::request_plan::{EncodedFamilyRequest, TransportResponseFraming};
use crate::streaming::ProviderStreamProjector;

use super::anthropic_stream_projector::AnthropicStreamProjector;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnthropicFamilyCodec;

impl ProviderFamilyCodec for AnthropicFamilyCodec {
    fn encode_task(
        &self,
        task: &TaskRequest,
        model: &str,
        response_mode: ResponseMode,
        family_options: Option<&FamilyOptions>,
    ) -> Result<EncodedFamilyRequest, AdapterError> {
        let family_options = parse_family_options(family_options)?;
        let encoded = encode_anthropic_request(task, model).map_err(map_anthropic_plan_error)?;
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
        decode_anthropic_response(&AnthropicDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        })
        .map_err(|error| refine_family_decode_error(&body, error))
    }

    fn decode_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        decode_anthropic_error(body)
    }

    fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector> {
        Box::<AnthropicStreamProjector>::default()
    }
}

fn parse_family_options(
    family_options: Option<&FamilyOptions>,
) -> Result<Option<&AnthropicFamilyOptions>, AdapterError> {
    match family_options {
        Some(FamilyOptions::Anthropic(options)) => Ok(Some(options)),
        Some(FamilyOptions::OpenAiCompatible(_)) => Err(AdapterError::new(
            AdapterErrorKind::Validation,
            ProviderKind::Anthropic,
            AdapterOperation::PlanRequest,
            "Anthropic codec received mismatched family native options",
        )),
        None => Ok(None),
    }
}

fn apply_family_options(
    body: &mut Value,
    family_options: Option<&AnthropicFamilyOptions>,
) -> Result<(), AdapterError> {
    let Some(body) = body.as_object_mut() else {
        return Err(AdapterError::new(
            AdapterErrorKind::ProtocolViolation,
            ProviderKind::Anthropic,
            AdapterOperation::PlanRequest,
            "Anthropic family request body must be an object",
        ));
    };

    if let Some(options) = family_options
        && let Some(thinking) = options.thinking.as_ref()
    {
        body.insert("thinking".to_string(), thinking.clone());
    }

    Ok(())
}

fn map_anthropic_plan_error(error: AnthropicFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_anthropic_family_error_kind(error.kind()),
        ProviderKind::Anthropic,
        AdapterOperation::PlanRequest,
        message,
        error,
    )
}

fn refine_family_decode_error(body: &Value, error: AnthropicFamilyError) -> AdapterError {
    let message = error.message().to_string();
    let mut error = AdapterError::with_source(
        map_anthropic_family_error_kind(error.kind()),
        ProviderKind::Anthropic,
        AdapterOperation::DecodeResponse,
        message,
        error,
    );

    if let Some(root) = body.as_object()
        && let Some(envelope) = parse_anthropic_error_value(root)
    {
        if let Some(provider_code) = envelope.error_type.as_deref() {
            error = error.with_provider_code(provider_code);
        }
        if let Some(request_id) = envelope.request_id.as_deref() {
            error = error.with_request_id(request_id);
        }
    }

    error
}

fn map_anthropic_family_error_kind(kind: AnthropicFamilyErrorKind) -> AdapterErrorKind {
    match kind {
        AnthropicFamilyErrorKind::Validation => AdapterErrorKind::Validation,
        AnthropicFamilyErrorKind::Encode => AdapterErrorKind::Encode,
        AnthropicFamilyErrorKind::Decode => AdapterErrorKind::Decode,
        AnthropicFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
        AnthropicFamilyErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        AnthropicFamilyErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}
