use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap};

use agent_core::{
    CanonicalStreamEvent, ContentPart, FinishReason, StreamOutputItemEnd, StreamOutputItemStart,
    ToolCall, Usage,
};
use serde_json::Value;

#[derive(Debug, Default)]
pub(super) struct StreamResponseState {
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
    pub(super) fn apply_events(&mut self, events: &[CanonicalStreamEvent]) -> Result<(), String> {
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
                CanonicalStreamEvent::OutputItemStarted { output_index, item } => {
                    self.apply_output_started(*output_index, item);
                }
                CanonicalStreamEvent::TextDelta {
                    output_index,
                    item_id,
                    delta,
                    ..
                } => {
                    let pending = self.ensure_pending_message(*output_index, item_id);
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
                    let pending = self.append_tool_call_delta(
                        *output_index,
                        Some(*tool_call_index),
                        item_id.as_deref(),
                        tool_call_id.as_deref(),
                        tool_name,
                    );
                    pending.arguments_json_text.push_str(delta);
                }
                CanonicalStreamEvent::OutputItemCompleted { output_index, item } => {
                    self.apply_output_completed(*output_index, item);
                }
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

    pub(super) fn failed_message(&self) -> Option<&str> {
        self.failed_message.as_deref()
    }

    pub(super) fn model_or_provider_fallback(&self, provider: agent_core::ProviderKind) -> String {
        self.model
            .clone()
            .unwrap_or_else(|| format!("{provider:?}").to_lowercase())
    }

    pub(super) fn response_id(&self) -> Option<String> {
        self.response_id.clone()
    }

    pub(super) fn usage(&self) -> Usage {
        self.usage.clone()
    }

    pub(super) fn finish_reason_or_other(&self) -> FinishReason {
        self.finish_reason.clone().unwrap_or(FinishReason::Other)
    }

    pub(super) fn into_content(mut self) -> Vec<ContentPart> {
        self.flush_pending_messages();
        self.flush_pending_tool_calls();
        self.completed_parts.sort_by_key(|part| part.sort_key);
        self.completed_parts
            .into_iter()
            .map(|part| part.part)
            .collect()
    }

    fn apply_output_started(&mut self, output_index: u32, item: &StreamOutputItemStart) {
        match item {
            StreamOutputItemStart::Message { item_id, .. } => {
                let _ = self.ensure_pending_message(output_index, item_id);
            }
            StreamOutputItemStart::ToolCall {
                item_id,
                tool_call_id,
                name,
            } => {
                self.start_tool_call(output_index, item_id, tool_call_id, name);
            }
        }
    }

    fn apply_output_completed(&mut self, output_index: u32, item: &StreamOutputItemEnd) {
        match item {
            StreamOutputItemEnd::Message { .. } => {
                if let Some(message) = self.pending_messages.remove(&output_index)
                    && !message.text.is_empty()
                {
                    self.completed_parts.push(CompletedContentPart {
                        sort_key: message.sort_key,
                        part: ContentPart::text(message.text),
                    });
                }
            }
            StreamOutputItemEnd::ToolCall {
                item_id,
                tool_call_id,
                name,
                arguments_json_text,
            } => {
                let key = self.resolve_tool_call_key(
                    output_index,
                    None,
                    item_id.as_deref(),
                    tool_call_id.as_deref(),
                );
                let pending = self.pending_tool_calls.remove(&key).unwrap_or_else(|| {
                    PendingToolCall::from_completion(
                        SortKey {
                            output_index,
                            ordinal: key.ordinal,
                        },
                        item_id.clone(),
                        tool_call_id.clone(),
                        name.clone(),
                        arguments_json_text.clone(),
                    )
                });
                self.push_completed_tool_call(pending.completed_with(
                    item_id.clone(),
                    tool_call_id.clone(),
                    name,
                    arguments_json_text,
                    self.completed_parts.len(),
                ));
            }
        }
    }

    fn ensure_pending_message(
        &mut self,
        output_index: u32,
        item_id: &Option<String>,
    ) -> &mut PendingMessage {
        let pending = if self.pending_messages.contains_key(&output_index) {
            match self.pending_messages.entry(output_index) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(PendingMessage {
                    sort_key: SortKey {
                        output_index,
                        ordinal: 0,
                    },
                    item_id: item_id.clone(),
                    text: String::new(),
                }),
            }
        } else {
            let sort_key = self.next_sort_key(output_index);
            match self.pending_messages.entry(output_index) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(PendingMessage {
                    sort_key,
                    item_id: item_id.clone(),
                    text: String::new(),
                }),
            }
        };
        if pending.item_id.is_none() {
            pending.item_id = item_id.clone();
        }
        pending
    }

    fn start_tool_call(
        &mut self,
        output_index: u32,
        item_id: &Option<String>,
        tool_call_id: &Option<String>,
        name: &str,
    ) {
        let sort_key = self.next_sort_key(output_index);
        let key = ToolCallKey::from_start(
            output_index,
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
                name: name.to_string(),
                arguments_json_text: String::new(),
                tool_call_index: None,
            });
    }

    fn append_tool_call_delta(
        &mut self,
        output_index: u32,
        tool_call_index: Option<u32>,
        item_id: Option<&str>,
        tool_call_id: Option<&str>,
        tool_name: &Option<String>,
    ) -> &mut PendingToolCall {
        let key = self.resolve_tool_call_key(output_index, tool_call_index, item_id, tool_call_id);
        let pending = if self.pending_tool_calls.contains_key(&key) {
            match self.pending_tool_calls.entry(key) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(PendingToolCall {
                    sort_key: SortKey {
                        output_index,
                        ordinal: 0,
                    },
                    item_id: item_id.map(ToOwned::to_owned),
                    tool_call_id: tool_call_id.map(ToOwned::to_owned),
                    name: tool_name.clone().unwrap_or_default(),
                    arguments_json_text: String::new(),
                    tool_call_index,
                }),
            }
        } else {
            let sort_key = SortKey {
                output_index,
                ordinal: key.ordinal,
            };
            match self.pending_tool_calls.entry(key) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(PendingToolCall {
                    sort_key: SortKey {
                        output_index: sort_key.output_index,
                        ordinal: sort_key.ordinal,
                    },
                    item_id: item_id.map(ToOwned::to_owned),
                    tool_call_id: tool_call_id.map(ToOwned::to_owned),
                    name: tool_name.clone().unwrap_or_default(),
                    arguments_json_text: String::new(),
                    tool_call_index,
                }),
            }
        };
        if pending.item_id.is_none() {
            pending.item_id = item_id.map(ToOwned::to_owned);
        }
        if pending.tool_call_id.is_none() {
            pending.tool_call_id = tool_call_id.map(ToOwned::to_owned);
        }
        if pending.name.is_empty() {
            pending.name = tool_name.clone().unwrap_or_default();
        }
        pending.tool_call_index = tool_call_index;
        pending
    }

    fn flush_pending_messages(&mut self) {
        for (_, pending) in std::mem::take(&mut self.pending_messages) {
            if !pending.text.is_empty() {
                self.completed_parts.push(CompletedContentPart {
                    sort_key: pending.sort_key,
                    part: ContentPart::text(pending.text),
                });
            }
        }
    }

    fn flush_pending_tool_calls(&mut self) {
        for (_, pending) in std::mem::take(&mut self.pending_tool_calls) {
            self.push_completed_tool_call(pending.into_completed(self.completed_parts.len()));
        }
    }

    fn push_completed_tool_call(&mut self, tool_call: CompletedToolCall) {
        self.completed_parts.push(CompletedContentPart {
            sort_key: tool_call.sort_key,
            part: ContentPart::ToolCall {
                tool_call: ToolCall {
                    id: tool_call.id,
                    name: tool_call.name,
                    arguments_json: tool_call.arguments_json,
                },
            },
        });
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

impl PendingToolCall {
    fn from_completion(
        sort_key: SortKey,
        item_id: Option<String>,
        tool_call_id: Option<String>,
        name: String,
        arguments_json_text: String,
    ) -> Self {
        Self {
            sort_key,
            item_id,
            tool_call_id,
            name,
            arguments_json_text,
            tool_call_index: None,
        }
    }

    fn completed_with(
        self,
        completed_item_id: Option<String>,
        completed_tool_call_id: Option<String>,
        completed_name: &str,
        completed_arguments_json_text: &str,
        completed_parts_len: usize,
    ) -> CompletedToolCall {
        let arguments_text = if self.arguments_json_text.is_empty() {
            completed_arguments_json_text.to_string()
        } else {
            self.arguments_json_text
        };
        CompletedToolCall {
            sort_key: self.sort_key,
            id: self
                .tool_call_id
                .or(completed_tool_call_id)
                .or_else(|| self.item_id.clone())
                .or(completed_item_id)
                .unwrap_or_else(|| format!("stream_tool_call_{completed_parts_len}")),
            name: if self.name.is_empty() {
                completed_name.to_string()
            } else {
                self.name
            },
            arguments_json: parse_arguments_json(&arguments_text),
        }
    }

    fn into_completed(self, completed_parts_len: usize) -> CompletedToolCall {
        CompletedToolCall {
            sort_key: self.sort_key,
            id: self
                .tool_call_id
                .or_else(|| self.item_id.clone())
                .unwrap_or_else(|| format!("stream_tool_call_{completed_parts_len}")),
            name: self.name,
            arguments_json: parse_arguments_json(&self.arguments_json_text),
        }
    }
}

#[derive(Debug)]
struct CompletedContentPart {
    sort_key: SortKey,
    part: ContentPart,
}

#[derive(Debug)]
struct CompletedToolCall {
    sort_key: SortKey,
    id: String,
    name: String,
    arguments_json: Value,
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

fn parse_arguments_json(arguments_json_text: &str) -> Value {
    serde_json::from_str::<Value>(arguments_json_text)
        .unwrap_or_else(|_| Value::String(arguments_json_text.to_string()))
}
