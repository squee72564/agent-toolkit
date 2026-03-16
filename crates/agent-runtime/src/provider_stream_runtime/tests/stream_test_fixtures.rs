use agent_core::{
    CanonicalStreamEnvelope, CanonicalStreamEvent, ProviderKind, Response, ResponseFormat,
};
use agent_transport::SseEvent;

use crate::provider_stream_runtime::{ProviderStreamRuntime, StreamRuntimeError};

pub(super) fn response_from_events(
    response_format: ResponseFormat,
    streamed_events: Vec<CanonicalStreamEvent>,
    final_events: Vec<CanonicalStreamEvent>,
) -> Result<Response, StreamRuntimeError> {
    ProviderStreamRuntime::response_from_events_for_test(
        ProviderKind::OpenAi,
        &response_format,
        Vec::new(),
        vec![CanonicalStreamEnvelope {
            raw: ProviderStreamRuntime::new(ProviderKind::OpenAi).wrap_sse_event(SseEvent {
                event: Some("test".to_string()),
                data: "{}".to_string(),
                id: Some("evt_test".to_string()),
                retry: None,
            }),
            canonical: streamed_events.clone(),
        }],
        &streamed_events,
        final_events,
    )
}
