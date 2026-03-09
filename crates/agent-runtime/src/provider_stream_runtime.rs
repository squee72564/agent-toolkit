use std::collections::{BTreeMap, HashMap};

use agent_core::{
    AssistantOutput, CanonicalStreamEnvelope, CanonicalStreamEvent, ContentPart, FinishReason,
    ProviderId, ProviderRawStreamEvent, Response, ResponseFormat, RuntimeWarning, ToolCall, Usage,
};
use agent_providers::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use agent_providers::streaming::ProviderStreamProjector;
use agent_transport::{HttpJsonResponse, HttpSseResponse, SseEvent};
use serde_json::{Value, json};

#[derive(Debug)]
pub(crate) struct ProviderStreamRuntime {
    provider: ProviderId,
    next_sequence: u64,
    state: StreamResponseState,
}

impl ProviderStreamRuntime {
    pub(crate) fn new(provider: ProviderId) -> Self {
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

        let state = std::mem::take(&mut self.state);
        let response_body = state.into_response(
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

#[derive(Debug, Default)]
struct StreamResponseState {
    model: Option<String>,
    response_id: Option<String>,
    usage: Usage,
    finish_reason: Option<FinishReason>,
    failed_message: Option<String>,
    next_item_ordinal: HashMap<u32, u32>,
    pending_messages: BTreeMap<u32, PendingMessage>,
    pending_tool_calls: BTreeMap<ToolCallKey, PendingToolCall>,
    completed_parts: Vec<CompletedContentPart>,
}

impl StreamResponseState {
    fn apply_events(&mut self, events: &[CanonicalStreamEvent]) -> Result<(), String> {
        for event in events {
            match event {
                CanonicalStreamEvent::ResponseStarted { model, response_id } => {
                    if self.model.is_none() {
                        self.model = model.clone();
                    }
                    if self.response_id.is_none() {
                        self.response_id = response_id.clone();
                    }
                }
                CanonicalStreamEvent::OutputItemStarted { output_index, item } => match item {
                    agent_core::StreamOutputItemStart::Message { item_id, .. } => {
                        if !self.pending_messages.contains_key(output_index) {
                            let sort_key = self.next_sort_key(*output_index);
                            self.pending_messages.insert(
                                *output_index,
                                PendingMessage {
                                    sort_key,
                                    item_id: item_id.clone(),
                                    text: String::new(),
                                },
                            );
                        }
                    }
                    agent_core::StreamOutputItemStart::ToolCall {
                        item_id,
                        tool_call_id,
                        name,
                    } => {
                        let sort_key = self.next_sort_key(*output_index);
                        let key = ToolCallKey::from_start(
                            *output_index,
                            sort_key.ordinal,
                            item_id.as_deref(),
                            tool_call_id.as_deref(),
                        );
                        self.pending_tool_calls
                            .entry(key)
                            .or_insert_with(|| PendingToolCall {
                                sort_key,
                                item_id: item_id.clone(),
                                tool_call_id: tool_call_id.clone(),
                                name: name.clone(),
                                arguments_json_text: String::new(),
                                tool_call_index: None,
                            });
                    }
                },
                CanonicalStreamEvent::TextDelta {
                    output_index,
                    item_id,
                    delta,
                    ..
                } => {
                    if !self.pending_messages.contains_key(output_index) {
                        let sort_key = self.next_sort_key(*output_index);
                        self.pending_messages.insert(
                            *output_index,
                            PendingMessage {
                                sort_key,
                                item_id: item_id.clone(),
                                text: String::new(),
                            },
                        );
                    }
                    let Some(pending) = self.pending_messages.get_mut(output_index) else {
                        continue;
                    };
                    if pending.item_id.is_none() {
                        pending.item_id = item_id.clone();
                    }
                    pending.text.push_str(delta);
                }
                CanonicalStreamEvent::ToolCallArgumentsDelta {
                    output_index,
                    tool_call_index,
                    item_id,
                    tool_call_id,
                    tool_name,
                    delta,
                } => {
                    let key = self.resolve_tool_call_key(
                        *output_index,
                        Some(*tool_call_index),
                        item_id.as_deref(),
                        tool_call_id.as_deref(),
                    );
                    let pending = self
                        .pending_tool_calls
                        .entry(key.clone())
                        .or_insert_with(|| PendingToolCall {
                            sort_key: SortKey {
                                output_index: *output_index,
                                ordinal: key.ordinal,
                            },
                            item_id: item_id.clone(),
                            tool_call_id: tool_call_id.clone(),
                            name: tool_name.clone().unwrap_or_default(),
                            arguments_json_text: String::new(),
                            tool_call_index: Some(*tool_call_index),
                        });
                    if pending.item_id.is_none() {
                        pending.item_id = item_id.clone();
                    }
                    if pending.tool_call_id.is_none() {
                        pending.tool_call_id = tool_call_id.clone();
                    }
                    if pending.name.is_empty() {
                        pending.name = tool_name.clone().unwrap_or_default();
                    }
                    pending.tool_call_index = Some(*tool_call_index);
                    pending.arguments_json_text.push_str(delta);
                }
                CanonicalStreamEvent::OutputItemCompleted { output_index, item } => match item {
                    agent_core::StreamOutputItemEnd::Message { .. } => {
                        if let Some(message) = self.pending_messages.remove(output_index)
                            && !message.text.is_empty()
                        {
                            self.completed_parts.push(CompletedContentPart {
                                sort_key: message.sort_key,
                                part: ContentPart::text(message.text),
                            });
                        }
                    }
                    agent_core::StreamOutputItemEnd::ToolCall {
                        item_id,
                        tool_call_id,
                        name,
                        arguments_json_text,
                    } => {
                        let key = self.resolve_tool_call_key(
                            *output_index,
                            None,
                            item_id.as_deref(),
                            tool_call_id.as_deref(),
                        );
                        let pending =
                            self.pending_tool_calls
                                .remove(&key)
                                .unwrap_or(PendingToolCall {
                                    sort_key: SortKey {
                                        output_index: *output_index,
                                        ordinal: key.ordinal,
                                    },
                                    item_id: item_id.clone(),
                                    tool_call_id: tool_call_id.clone(),
                                    name: name.clone(),
                                    arguments_json_text: String::new(),
                                    tool_call_index: None,
                                });
                        let combined_arguments = if pending.arguments_json_text.is_empty() {
                            arguments_json_text.clone()
                        } else {
                            pending.arguments_json_text
                        };
                        let tool_call_id = pending
                            .tool_call_id
                            .or_else(|| tool_call_id.clone())
                            .or_else(|| pending.item_id.clone())
                            .unwrap_or_else(|| {
                                format!("stream_tool_call_{}", self.completed_parts.len())
                            });
                        let arguments_json = serde_json::from_str::<Value>(&combined_arguments)
                            .unwrap_or_else(|_| Value::String(combined_arguments.clone()));
                        self.completed_parts.push(CompletedContentPart {
                            sort_key: pending.sort_key,
                            part: ContentPart::ToolCall {
                                tool_call: ToolCall {
                                    id: tool_call_id,
                                    name: if pending.name.is_empty() {
                                        name.clone()
                                    } else {
                                        pending.name
                                    },
                                    arguments_json,
                                },
                            },
                        });
                    }
                },
                CanonicalStreamEvent::UsageUpdated { usage } => {
                    self.usage = usage.clone();
                }
                CanonicalStreamEvent::Completed { finish_reason } => {
                    self.finish_reason = Some(finish_reason.clone());
                }
                CanonicalStreamEvent::Failed { message } => {
                    self.failed_message = Some(message.clone());
                }
            }
        }

        Ok(())
    }

    fn into_response(
        mut self,
        provider: ProviderId,
        response_format: &ResponseFormat,
        mut prepended_warnings: Vec<RuntimeWarning>,
        transcript: Vec<CanonicalStreamEnvelope>,
        final_events: Vec<CanonicalStreamEvent>,
    ) -> Result<Response, StreamRuntimeError> {
        if let Some(message) = self.failed_message {
            return Err(StreamRuntimeError::Adapter {
                error: AdapterError::new(
                    AdapterErrorKind::Upstream,
                    provider,
                    AdapterOperation::FinalizeStream,
                    message,
                ),
                request_id: None,
                status_code: None,
            });
        }

        for (_, pending) in self.pending_messages {
            if !pending.text.is_empty() {
                self.completed_parts.push(CompletedContentPart {
                    sort_key: pending.sort_key,
                    part: ContentPart::text(pending.text),
                });
            }
        }
        for (_, pending) in self.pending_tool_calls {
            let tool_call_id = pending
                .tool_call_id
                .or_else(|| pending.item_id.clone())
                .unwrap_or_else(|| format!("stream_tool_call_{}", self.completed_parts.len()));
            let arguments_json = serde_json::from_str::<Value>(&pending.arguments_json_text)
                .unwrap_or_else(|_| Value::String(pending.arguments_json_text.clone()));
            self.completed_parts.push(CompletedContentPart {
                sort_key: pending.sort_key,
                part: ContentPart::ToolCall {
                    tool_call: ToolCall {
                        id: tool_call_id,
                        name: pending.name,
                        arguments_json,
                    },
                },
            });
        }

        self.completed_parts.sort_by_key(|part| part.sort_key);
        let content: Vec<ContentPart> = self
            .completed_parts
            .into_iter()
            .map(|part| part.part)
            .collect();
        let structured_output = decode_stream_structured_output(response_format, &content);
        prepended_warnings.extend(stream_structured_output_warnings(
            response_format,
            structured_output.as_ref(),
            &content,
        ));

        Ok(Response {
            output: AssistantOutput {
                content,
                structured_output,
            },
            usage: self.usage,
            model: self
                .model
                .unwrap_or_else(|| format!("{provider:?}").to_lowercase()),
            raw_provider_response: Some(json!({
                "transport": "sse",
                "response_id": self.response_id,
                "events": transcript,
                "final_events": final_events,
            })),
            finish_reason: self.finish_reason.unwrap_or(FinishReason::Other),
            warnings: prepended_warnings,
        })
    }

    fn next_sort_key(&mut self, output_index: u32) -> SortKey {
        let ordinal = self.next_item_ordinal.entry(output_index).or_default();
        let current = *ordinal;
        *ordinal = ordinal.saturating_add(1);
        SortKey {
            output_index,
            ordinal: current,
        }
    }

    fn resolve_tool_call_key(
        &self,
        output_index: u32,
        tool_call_index: Option<u32>,
        item_id: Option<&str>,
        tool_call_id: Option<&str>,
    ) -> ToolCallKey {
        if let Some(key) = self.pending_tool_calls.keys().find(|key| {
            key.output_index == output_index
                && tool_call_id.is_some_and(|id| key.tool_call_id.as_deref() == Some(id))
        }) {
            return key.clone();
        }
        if let Some(key) = self.pending_tool_calls.keys().find(|key| {
            key.output_index == output_index
                && item_id.is_some_and(|id| key.item_id.as_deref() == Some(id))
        }) {
            return key.clone();
        }
        if let Some(tool_call_index) = tool_call_index
            && let Some(key) = self.pending_tool_calls.keys().find(|key| {
                key.output_index == output_index && key.tool_call_index == Some(tool_call_index)
            })
        {
            return key.clone();
        }

        ToolCallKey {
            output_index,
            ordinal: tool_call_index.unwrap_or(0),
            item_id: item_id.map(ToOwned::to_owned),
            tool_call_id: tool_call_id.map(ToOwned::to_owned),
            tool_call_index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    output_index: u32,
    ordinal: u32,
}

#[derive(Debug)]
struct PendingMessage {
    sort_key: SortKey,
    item_id: Option<String>,
    text: String,
}

#[derive(Debug)]
struct PendingToolCall {
    sort_key: SortKey,
    item_id: Option<String>,
    tool_call_id: Option<String>,
    name: String,
    arguments_json_text: String,
    tool_call_index: Option<u32>,
}

#[derive(Debug)]
struct CompletedContentPart {
    sort_key: SortKey,
    part: ContentPart,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ToolCallKey {
    output_index: u32,
    ordinal: u32,
    item_id: Option<String>,
    tool_call_id: Option<String>,
    tool_call_index: Option<u32>,
}

impl ToolCallKey {
    fn from_start(
        output_index: u32,
        ordinal: u32,
        item_id: Option<&str>,
        tool_call_id: Option<&str>,
    ) -> Self {
        Self {
            output_index,
            ordinal,
            item_id: item_id.map(ToOwned::to_owned),
            tool_call_id: tool_call_id.map(ToOwned::to_owned),
            tool_call_index: None,
        }
    }
}

fn decode_stream_structured_output(
    response_format: &ResponseFormat,
    content: &[ContentPart],
) -> Option<Value> {
    match response_format {
        ResponseFormat::Text => None,
        ResponseFormat::JsonObject | ResponseFormat::JsonSchema { .. } => {
            let text = content.iter().find_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })?;
            let parsed = serde_json::from_str::<Value>(text).ok()?;
            parsed.is_object().then_some(parsed)
        }
    }
}

fn stream_structured_output_warnings(
    response_format: &ResponseFormat,
    structured_output: Option<&Value>,
    content: &[ContentPart],
) -> Vec<RuntimeWarning> {
    match response_format {
        ResponseFormat::Text => Vec::new(),
        ResponseFormat::JsonObject | ResponseFormat::JsonSchema { .. } => {
            let Some(text) = content.iter().find_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            }) else {
                return Vec::new();
            };
            match serde_json::from_str::<Value>(text) {
                Ok(value) if value.is_object() => {
                    let _ = structured_output;
                    Vec::new()
                }
                Ok(_) => vec![RuntimeWarning {
                    code: "runtime.stream.structured_output_not_object".to_string(),
                    message: "streamed structured output was not a JSON object".to_string(),
                }],
                Err(error) => vec![RuntimeWarning {
                    code: "runtime.stream.structured_output_parse_failed".to_string(),
                    message: format!("failed to parse streamed structured output: {error}"),
                }],
            }
        }
    }
}
