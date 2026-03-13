use agent_core::{
    NativeOptions, OpenAiCompatibleOptions, OpenAiOptions, ProviderKind, ProviderOptions, Request,
    ResponseMode, TaskRequest,
};
use agent_transport::HttpRequestOptions;
use reqwest::{Method, header::HeaderMap};
use serde_json::Value;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_family::encode::encode_openai_request;
use crate::openai_family::{OpenAiFamilyError, OpenAiFamilyErrorKind};
use crate::request_plan::{EncodedFamilyRequest, TransportResponseFraming};

#[derive(Debug, Clone, Default, PartialEq)]
struct OpenAiNativeOptionsOverrides {
    parallel_tool_calls: Option<bool>,
    reasoning: Option<Value>,
    service_tier: Option<String>,
    store: Option<bool>,
}

impl OpenAiNativeOptionsOverrides {
    fn from_options(
        provider: ProviderKind,
        family_options: Option<&OpenAiCompatibleOptions>,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<Self, AdapterError> {
        let mut overrides = Self::default();

        if let Some(options) = family_options {
            overrides.parallel_tool_calls = options.parallel_tool_calls;
            overrides.reasoning = options.reasoning.clone();
        }

        if let Some(provider_options) = provider_options {
            match (provider, provider_options) {
                (
                    ProviderKind::OpenAi,
                    ProviderOptions::OpenAi(OpenAiOptions {
                        service_tier,
                        store,
                    }),
                ) => {
                    overrides.service_tier = service_tier.clone();
                    overrides.store = *store;
                }
                (ProviderKind::GenericOpenAiCompatible, _) => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Validation,
                        ProviderKind::GenericOpenAiCompatible,
                        AdapterOperation::PlanRequest,
                        "generic OpenAI-compatible adapter does not support provider-scoped native options"
                            .to_string(),
                    ));
                }
                (_, other) => {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Validation,
                        provider,
                        AdapterOperation::PlanRequest,
                        format!(
                            "OpenAI-compatible adapter received mismatched provider native options for {:?}",
                            other.provider_kind()
                        ),
                    ));
                }
            }
        }

        Ok(overrides)
    }

    fn apply(
        &self,
        provider: ProviderKind,
        request: &mut EncodedFamilyRequest,
    ) -> Result<(), AdapterError> {
        let Some(body) = request.body.as_object_mut() else {
            return Err(AdapterError::new(
                AdapterErrorKind::ProtocolViolation,
                provider,
                AdapterOperation::PlanRequest,
                "OpenAI family request body must be an object".to_string(),
            ));
        };

        if let Some(parallel_tool_calls) = self.parallel_tool_calls {
            body.insert(
                "parallel_tool_calls".to_string(),
                Value::Bool(parallel_tool_calls),
            );
        }
        if let Some(reasoning) = self.reasoning.as_ref() {
            body.insert("reasoning".to_string(), reasoning.clone());
        }
        if let Some(service_tier) = self.service_tier.as_ref() {
            body.insert(
                "service_tier".to_string(),
                Value::String(service_tier.clone()),
            );
        }
        if let Some(store) = self.store {
            body.insert("store".to_string(), Value::Bool(store));
        }

        Ok(())
    }
}

pub(crate) fn encode_family_request(
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
) -> Result<EncodedFamilyRequest, AdapterError> {
    let mut request = Request::from(task.clone());
    request.model_id = model.to_string();
    request.stream = response_mode == ResponseMode::Streaming;

    let encoded = encode_openai_request(request).map_err(map_openai_plan_error)?;
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
    provider: ProviderKind,
    request: &mut EncodedFamilyRequest,
    family_options: Option<&OpenAiCompatibleOptions>,
    provider_options: Option<&ProviderOptions>,
) -> Result<(), AdapterError> {
    OpenAiNativeOptionsOverrides::from_options(provider, family_options, provider_options)?
        .apply(provider, request)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn plan_request(
    req: Request,
    provider: ProviderKind,
    native_options: Option<&NativeOptions>,
) -> Result<crate::request_plan::ProviderRequestPlan, AdapterError> {
    let mut request = encode_family_request(
        &req.task_request(),
        &req.model_id,
        if req.stream {
            ResponseMode::Streaming
        } else {
            ResponseMode::NonStreaming
        },
    )
    .map_err(|mut error| {
        error.provider = provider;
        error
    })?;
    let family_options = native_options
        .and_then(|native| native.family.as_ref())
        .and_then(|family| match family {
            agent_core::FamilyOptions::OpenAiCompatible(options) => Some(options),
            _ => None,
        });
    let provider_options = native_options.and_then(|native| native.provider.as_ref());
    apply_provider_overlay(provider, &mut request, family_options, provider_options)?;
    Ok(request.into())
}

fn map_openai_plan_error(error: OpenAiFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::OpenAi,
        AdapterOperation::PlanRequest,
        message,
        error,
    )
}

fn map_spec_error_kind(kind: OpenAiFamilyErrorKind) -> AdapterErrorKind {
    match kind {
        OpenAiFamilyErrorKind::Validation => AdapterErrorKind::Validation,
        OpenAiFamilyErrorKind::Encode => AdapterErrorKind::Encode,
        OpenAiFamilyErrorKind::Decode => AdapterErrorKind::Decode,
        OpenAiFamilyErrorKind::Upstream => AdapterErrorKind::Upstream,
        OpenAiFamilyErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        OpenAiFamilyErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}
