use agent_core::{
    AnthropicOptions, FamilyOptions, NativeOptions, ProviderId, ProviderOptions, Request,
};
use agent_transport::HttpRequestOptions;
use serde_json::Value;

use crate::anthropic_family::{AnthropicFamilyError, AnthropicFamilyErrorKind};
use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::request_plan::{ProviderRequestPlan, ProviderResponseKind, ProviderTransportKind};

#[derive(Debug, Clone, Default, PartialEq)]
struct AnthropicNativeOptionsOverrides {
    thinking: Option<Value>,
    top_k: Option<u32>,
}

impl AnthropicNativeOptionsOverrides {
    fn from_native_options(native_options: Option<&NativeOptions>) -> Result<Self, AdapterError> {
        let Some(native_options) = native_options else {
            return Ok(Self::default());
        };

        let mut overrides = Self::default();

        if let Some(FamilyOptions::Anthropic(options)) = native_options.family.as_ref() {
            overrides.thinking = options.thinking.clone();
        }

        if let Some(provider_options) = native_options.provider.as_ref() {
            let ProviderOptions::Anthropic(AnthropicOptions { top_k }) = provider_options else {
                return Err(AdapterError::new(
                    AdapterErrorKind::Validation,
                    ProviderId::Anthropic,
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

    fn apply(&self, plan: &mut ProviderRequestPlan) -> Result<(), AdapterError> {
        let Some(body) = plan.body.as_object_mut() else {
            return Err(AdapterError::new(
                AdapterErrorKind::ProtocolViolation,
                ProviderId::Anthropic,
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

pub(crate) fn plan_request(
    req: Request,
    native_options: Option<&NativeOptions>,
) -> Result<ProviderRequestPlan, AdapterError> {
    let is_stream = req.stream;
    let overrides = AnthropicNativeOptionsOverrides::from_native_options(native_options)?;
    let encoded = crate::anthropic_family::encode::encode_anthropic_request(req)
        .map_err(map_anthropic_plan_error)?;
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
    overrides.apply(&mut plan)?;
    Ok(plan)
}

fn map_anthropic_plan_error(error: AnthropicFamilyError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::Anthropic,
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
