use agent_core::{
    FamilyOptions, NativeOptions, OpenAiOptions, ProviderId, ProviderKind, ProviderOptions, Request,
};
use agent_transport::HttpRequestOptions;
use serde_json::Value;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_family::encode::encode_openai_request;
use crate::openai_family::{OpenAiFamilyError, OpenAiFamilyErrorKind};
use crate::request_plan::{ProviderRequestPlan, ProviderResponseKind, ProviderTransportKind};

#[derive(Debug, Clone, Default, PartialEq)]
struct OpenAiNativeOptionsOverrides {
    parallel_tool_calls: Option<bool>,
    reasoning: Option<Value>,
    service_tier: Option<String>,
    store: Option<bool>,
}

impl OpenAiNativeOptionsOverrides {
    fn from_native_options(
        provider: ProviderKind,
        native_options: Option<&NativeOptions>,
    ) -> Result<Self, AdapterError> {
        let Some(native_options) = native_options else {
            return Ok(Self::default());
        };

        let mut overrides = Self::default();

        if let Some(FamilyOptions::OpenAiCompatible(options)) = native_options.family.as_ref() {
            overrides.parallel_tool_calls = options.parallel_tool_calls;
            overrides.reasoning = options.reasoning.clone();
        }

        if let Some(provider_options) = native_options.provider.as_ref() {
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
                        ProviderId::GenericOpenAiCompatible,
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
        plan: &mut ProviderRequestPlan,
    ) -> Result<(), AdapterError> {
        let Some(body) = plan.body.as_object_mut() else {
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

pub(crate) fn plan_request(
    req: Request,
    provider: ProviderKind,
    native_options: Option<&NativeOptions>,
) -> Result<ProviderRequestPlan, AdapterError> {
    let is_stream = req.stream;
    let overrides = OpenAiNativeOptionsOverrides::from_native_options(provider, native_options)?;
    let encoded = encode_openai_request(req).map_err(map_openai_plan_error)?;
    let mut body = encoded.body;
    if is_stream {
        body["stream"] = serde_json::Value::Bool(true);
    }

    let mut plan = ProviderRequestPlan {
        body,
        warnings: encoded.warnings,
        transport_kind: if is_stream {
            ProviderTransportKind::HttpSse
        } else {
            ProviderTransportKind::HttpJson
        },
        response_kind: if is_stream {
            ProviderResponseKind::RawProviderStream
        } else {
            ProviderResponseKind::JsonBody
        },
        endpoint_path_override: None,
        request_options: if is_stream {
            HttpRequestOptions::sse_defaults()
        } else {
            HttpRequestOptions::json_defaults().with_allow_error_status(true)
        },
    };
    overrides.apply(provider, &mut plan)?;
    Ok(plan)
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
