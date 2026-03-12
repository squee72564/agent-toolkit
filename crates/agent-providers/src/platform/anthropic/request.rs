use agent_core::Request;
use agent_transport::HttpRequestOptions;

use crate::anthropic_family::{AnthropicFamilyError, AnthropicFamilyErrorKind};
use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::request_plan::{ProviderRequestPlan, ProviderResponseKind, ProviderTransportKind};

pub(crate) fn plan_request(req: Request) -> Result<ProviderRequestPlan, AdapterError> {
    let is_stream = req.stream;
    let encoded = crate::anthropic_family::encode::encode_anthropic_request(req)
        .map_err(map_anthropic_plan_error)?;
    let mut body = encoded.body;
    if is_stream {
        body["stream"] = serde_json::Value::Bool(true);
    }

    Ok(ProviderRequestPlan {
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
    })
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
