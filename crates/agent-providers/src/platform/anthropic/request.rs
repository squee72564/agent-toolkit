use agent_core::{
    AnthropicFamilyOptions, AnthropicOptions, NativeOptions, ProviderOptions, ResponseMode,
    TaskRequest,
};
use agent_transport::HttpRequestOptions;
use reqwest::{Method, header::HeaderMap};
use serde_json::Value;

use crate::anthropic_family::{AnthropicFamilyError, AnthropicFamilyErrorKind};
use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::request_plan::{EncodedFamilyRequest, TransportResponseFraming};

#[derive(Debug, Clone, Default, PartialEq)]
struct AnthropicNativeOptionsOverrides {
    thinking: Option<Value>,
    top_k: Option<u32>,
}

impl AnthropicNativeOptionsOverrides {
    fn from_options(
        family_options: Option<&AnthropicFamilyOptions>,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<Self, AdapterError> {
        let mut overrides = Self::default();

        if let Some(options) = family_options {
            overrides.thinking = options.thinking.clone();
        }

        if let Some(provider_options) = provider_options {
            let ProviderOptions::Anthropic(AnthropicOptions { top_k }) = provider_options else {
                return Err(AdapterError::new(
                    AdapterErrorKind::Validation,
                    agent_core::ProviderKind::Anthropic,
                    AdapterOperation::PlanRequest,
                    format!(
                        "Anthropic adapter received mismatched provider native options for {:?}",
                        provider_options.provider_kind()
                    ),
                ));
            };
            overrides.top_k = *top_k;
        }

        Ok(overrides)
    }

    fn apply(&self, request: &mut EncodedFamilyRequest) -> Result<(), AdapterError> {
        let Some(body) = request.body.as_object_mut() else {
            return Err(AdapterError::new(
                AdapterErrorKind::ProtocolViolation,
                agent_core::ProviderKind::Anthropic,
                AdapterOperation::PlanRequest,
                "Anthropic family request body must be an object".to_string(),
            ));
        };

        if let Some(thinking) = self.thinking.as_ref() {
            body.insert("thinking".to_string(), thinking.clone());
        }
        if let Some(top_k) = self.top_k {
            body.insert("top_k".to_string(), Value::from(top_k));
        }

        Ok(())
    }
}

pub(crate) fn encode_family_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
) -> Result<EncodedFamilyRequest, AdapterError> {
    let encoded = crate::anthropic_family::encode::encode_anthropic_request(task, model)
        .map_err(map_anthropic_plan_error)?;
    let mut body = encoded.body;
    if response_mode == ResponseMode::Streaming {
        body["stream"] = Value::Bool(true);
    }

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

pub(crate) fn apply_provider_overlay(
    request: &mut EncodedFamilyRequest,
    family_options: Option<&AnthropicFamilyOptions>,
    provider_options: Option<&ProviderOptions>,
) -> Result<(), AdapterError> {
    AnthropicNativeOptionsOverrides::from_options(family_options, provider_options)?.apply(request)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn plan_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
    native_options: Option<&NativeOptions>,
) -> Result<crate::request_plan::ProviderRequestPlan, AdapterError> {
    let mut request = encode_family_request(task, model, response_mode)?;
    let family_options = native_options
        .and_then(|native| native.family.as_ref())
        .and_then(|family| match family {
            agent_core::FamilyOptions::Anthropic(options) => Some(options),
            _ => None,
        });
    let provider_options = native_options.and_then(|native| native.provider.as_ref());
    apply_provider_overlay(&mut request, family_options, provider_options)?;
    Ok(request.into())
}

fn map_anthropic_plan_error(error: AnthropicFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderKind::Anthropic,
        AdapterOperation::PlanRequest,
        message,
        error,
    )
}

fn map_spec_error_kind(kind: AnthropicFamilyErrorKind) -> AdapterErrorKind {
    match kind {
        AnthropicFamilyErrorKind::Validation => AdapterErrorKind::Validation,
        AnthropicFamilyErrorKind::Encode => AdapterErrorKind::Encode,
        AnthropicFamilyErrorKind::Decode => AdapterErrorKind::Decode,
        AnthropicFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
        AnthropicFamilyErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        AnthropicFamilyErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}
