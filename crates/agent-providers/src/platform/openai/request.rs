use agent_core::Request;
use agent_transport::HttpRequestOptions;

use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_family::encode::encode_openai_request;
use crate::openai_family::{OpenAiSpecError, OpenAiSpecErrorKind};
use crate::request_plan::{ProviderRequestPlan, ProviderResponseKind, ProviderTransportKind};

pub(crate) fn plan_request(req: Request) -> Result<ProviderRequestPlan, AdapterError> {
    let is_stream = req.stream;
    let encoded = encode_openai_request(req).map_err(map_openai_plan_error)?;
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

fn map_openai_plan_error(error: OpenAiSpecError) -> AdapterError {
    let message = error.message().to_string();
    AdapterError::with_source(
        map_spec_error_kind(error.kind()),
        agent_core::ProviderId::OpenAi,
        AdapterOperation::PlanRequest,
        message,
        error,
    )
}

fn map_spec_error_kind(kind: OpenAiSpecErrorKind) -> AdapterErrorKind {
    match kind {
        OpenAiSpecErrorKind::Validation => AdapterErrorKind::Validation,
        OpenAiSpecErrorKind::Encode => AdapterErrorKind::Encode,
        OpenAiSpecErrorKind::Decode => AdapterErrorKind::Decode,
        OpenAiSpecErrorKind::Upstream => AdapterErrorKind::Upstream,
        OpenAiSpecErrorKind::ProtocolViolation => AdapterErrorKind::ProtocolViolation,
        OpenAiSpecErrorKind::UnsupportedFeature => AdapterErrorKind::UnsupportedFeature,
    }
}
