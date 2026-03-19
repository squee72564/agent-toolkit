use agent_core::{
    AssistantOutput, CanonicalStreamEnvelope, CanonicalStreamEvent, ProviderKind, Response,
    ResponseFormat, RuntimeWarning,
};
use agent_providers::{AdapterError, AdapterErrorKind, AdapterOperation};
use serde_json::json;

use crate::provider_stream_runtime::StreamRuntimeError;

use super::reducer::StreamResponseState;
use super::structured_output::decode_structured_output_payload;

pub(super) fn finalize_stream_response(
    state: StreamResponseState,
    provider: ProviderKind,
    response_format: &ResponseFormat,
    mut prepended_warnings: Vec<RuntimeWarning>,
    transcript: Vec<CanonicalStreamEnvelope>,
    final_events: Vec<CanonicalStreamEvent>,
) -> Result<Response, StreamRuntimeError> {
    if let Some(message) = state.failed_message() {
        return Err(StreamRuntimeError::Adapter {
            error: AdapterError::new(
                AdapterErrorKind::Upstream,
                provider,
                AdapterOperation::FinalizeStream,
                message.to_string(),
            ),
            request_id: None,
            status_code: None,
        });
    }

    let model = state.model_or_provider_fallback(provider);
    let usage = state.usage();
    let finish_reason = state.finish_reason_or_other();
    let response_id = state.response_id();
    let content = state.into_content();
    let structured = decode_structured_output_payload(response_format, &content);
    prepended_warnings.extend(structured.warnings);

    Ok(Response {
        output: AssistantOutput {
            content,
            structured_output: structured.structured_output,
        },
        usage,
        model,
        raw_provider_response: Some(json!({
            "transport": "sse",
            "response_id": response_id,
            "events": transcript,
            "final_events": final_events,
        })),
        finish_reason,
        warnings: prepended_warnings,
    })
}
