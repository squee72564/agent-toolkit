use agent_core::{
    CanonicalStreamEnvelope, CanonicalStreamEvent, ProviderKind, ProviderRawStreamEvent, Response,
    ResponseFormat, RuntimeWarning,
};
use agent_providers::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use agent_providers::streaming::ProviderStreamProjector;
use agent_transport::{HttpJsonResponse, HttpSseResponse, SseEvent};
use serde_json::Value;

mod finalize;
mod reducer;
mod structured_output;

use self::finalize::finalize_stream_response;
use self::reducer::StreamResponseState;

#[derive(Debug)]
pub(crate) struct ProviderStreamRuntime {
    provider: ProviderKind,
    next_sequence: u64,
    state: StreamResponseState,
}

impl ProviderStreamRuntime {
    pub(crate) fn new(provider: ProviderKind) -> Self {
        Self {
            provider,
            next_sequence: 0,
            state: StreamResponseState::default(),
        }
    }

    pub(crate) fn wrap_sse_event(&mut self, event: SseEvent) -> ProviderRawStreamEvent {
        self.next_sequence = self.next_sequence.saturating_add(1);
        ProviderRawStreamEvent::from_sse(
            self.provider,
            self.next_sequence,
            event.event,
            event.id,
            event.retry,
            event.data,
        )
    }

    pub(crate) async fn next_envelope(
        &mut self,
        response: &mut HttpSseResponse,
        projector: &mut dyn ProviderStreamProjector,
        operation: AdapterOperation,
    ) -> Result<Option<CanonicalStreamEnvelope>, StreamRuntimeError> {
        let Some(sse_event) =
            response
                .stream
                .next_event()
                .await
                .map_err(|error| StreamRuntimeError::Transport {
                    error,
                    request_id: response.head.request_id.clone(),
                    status_code: Some(response.head.status.as_u16()),
                })?
        else {
            return Ok(None);
        };

        let raw = self.wrap_sse_event(sse_event);
        let canonical =
            projector
                .project(raw.clone())
                .map_err(|error| StreamRuntimeError::Adapter {
                    error,
                    request_id: response.head.request_id.clone(),
                    status_code: Some(response.head.status.as_u16()),
                })?;
        self.apply_projected_events(
            &canonical,
            response.head.request_id.clone(),
            Some(response.head.status.as_u16()),
            operation,
        )?;
        Ok(Some(CanonicalStreamEnvelope { raw, canonical }))
    }

    pub(crate) fn finalize_response(
        &mut self,
        response: HttpSseResponse,
        projector: &mut dyn ProviderStreamProjector,
        response_format: &ResponseFormat,
        prepended_warnings: Vec<RuntimeWarning>,
        transcript: Vec<CanonicalStreamEnvelope>,
        operation: AdapterOperation,
    ) -> Result<(Response, HttpJsonResponse), StreamRuntimeError> {
        let final_events = projector
            .finish()
            .map_err(|error| StreamRuntimeError::Adapter {
                error,
                request_id: response.head.request_id.clone(),
                status_code: Some(response.head.status.as_u16()),
            })?;
        self.apply_projected_events(
            &final_events,
            response.head.request_id.clone(),
            Some(response.head.status.as_u16()),
            operation,
        )?;

        let response_body = finalize_stream_response(
            std::mem::take(&mut self.state),
            self.provider,
            response_format,
            prepended_warnings,
            transcript,
            final_events,
        )?;
        let http_response = HttpJsonResponse {
            head: response.head,
            body: response_body
                .raw_provider_response
                .clone()
                .unwrap_or(Value::Null),
        };

        Ok((response_body, http_response))
    }

    fn apply_projected_events(
        &mut self,
        events: &[CanonicalStreamEvent],
        request_id: Option<String>,
        status_code: Option<u16>,
        operation: AdapterOperation,
    ) -> Result<(), StreamRuntimeError> {
        self.state
            .apply_events(events)
            .map_err(|message| StreamRuntimeError::Adapter {
                error: AdapterError::new(
                    AdapterErrorKind::ProtocolViolation,
                    self.provider,
                    operation,
                    message,
                ),
                request_id,
                status_code,
            })
    }

    #[cfg(test)]
    pub(crate) fn response_from_events_for_test(
        provider: ProviderKind,
        response_format: &ResponseFormat,
        prepended_warnings: Vec<RuntimeWarning>,
        transcript: Vec<CanonicalStreamEnvelope>,
        streamed_events: &[CanonicalStreamEvent],
        final_events: Vec<CanonicalStreamEvent>,
    ) -> Result<Response, StreamRuntimeError> {
        let mut runtime = Self::new(provider);
        runtime
            .state
            .apply_events(streamed_events)
            .map_err(|message| StreamRuntimeError::Adapter {
                error: AdapterError::new(
                    AdapterErrorKind::ProtocolViolation,
                    provider,
                    AdapterOperation::ProjectStreamEvent,
                    message,
                ),
                request_id: None,
                status_code: None,
            })?;
        runtime
            .state
            .apply_events(&final_events)
            .map_err(|message| StreamRuntimeError::Adapter {
                error: AdapterError::new(
                    AdapterErrorKind::ProtocolViolation,
                    provider,
                    AdapterOperation::FinalizeStream,
                    message,
                ),
                request_id: None,
                status_code: None,
            })?;

        finalize_stream_response(
            runtime.state,
            provider,
            response_format,
            prepended_warnings,
            transcript,
            final_events,
        )
    }
}

#[derive(Debug)]
pub(crate) enum StreamRuntimeError {
    Adapter {
        error: AdapterError,
        request_id: Option<String>,
        status_code: Option<u16>,
    },
    Transport {
        error: agent_transport::TransportError,
        request_id: Option<String>,
        status_code: Option<u16>,
    },
}
