use agent_core::{CanonicalStreamEvent, MessageRole, StreamOutputItemEnd, StreamOutputItemStart};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::types::{
    OpenAiFunctionCallOutputItem, OpenAiMessageOutputItem, OpenAiReasoningOutputItem,
    OpenAiRefusalOutputItem,
};

pub(crate) fn project_output_item_added(value: &Value) -> Option<CanonicalStreamEvent> {
    let output_index = json_u32(value, "output_index")?;
    let item = value.get("item")?;

    match json_str(item, "type") {
        Some("message") => {
            let item = deserialize_wire::<OpenAiMessageOutputItem>(item)?;
            Some(CanonicalStreamEvent::OutputItemStarted {
                output_index,
                item: StreamOutputItemStart::Message {
                    item_id: item.id,
                    role: parse_message_role(item.role.as_deref())
                        .unwrap_or(MessageRole::Assistant),
                },
            })
        }
        Some("function_call") => {
            let item = deserialize_wire::<OpenAiFunctionCallOutputItem>(item)?;
            let name = item.name?;

            Some(CanonicalStreamEvent::OutputItemStarted {
                output_index,
                item: StreamOutputItemStart::ToolCall {
                    item_id: item.id,
                    tool_call_id: item.call_id,
                    name,
                },
            })
        }
        Some("reasoning") => {
            let _ = deserialize_wire::<OpenAiReasoningOutputItem>(item)?;
            None
        }
        Some("refusal") => {
            let _ = deserialize_wire::<OpenAiRefusalOutputItem>(item)?;
            None
        }
        _ => None,
    }
}

pub(crate) fn project_output_item_done(value: &Value) -> Option<CanonicalStreamEvent> {
    let output_index = json_u32(value, "output_index")?;
    let item = value.get("item")?;

    match json_str(item, "type") {
        Some("message") => {
            let item = deserialize_wire::<OpenAiMessageOutputItem>(item)?;
            Some(CanonicalStreamEvent::OutputItemCompleted {
                output_index,
                item: StreamOutputItemEnd::Message { item_id: item.id },
            })
        }
        Some("function_call") => {
            let item = deserialize_wire::<OpenAiFunctionCallOutputItem>(item)?;
            let name = item.name?;

            Some(CanonicalStreamEvent::OutputItemCompleted {
                output_index,
                item: StreamOutputItemEnd::ToolCall {
                    item_id: item.id,
                    tool_call_id: item.call_id,
                    name,
                    arguments_json_text: item.arguments.unwrap_or_default(),
                },
            })
        }
        Some("reasoning") => {
            let _ = deserialize_wire::<OpenAiReasoningOutputItem>(item)?;
            None
        }
        Some("refusal") => {
            let _ = deserialize_wire::<OpenAiRefusalOutputItem>(item)?;
            None
        }
        _ => None,
    }
}

fn deserialize_wire<T>(value: &Value) -> Option<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.clone()).ok()
}

fn json_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn json_u32(value: &Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn parse_message_role(role: Option<&str>) -> Option<MessageRole> {
    match role {
        Some("system") => Some(MessageRole::System),
        Some("user") => Some(MessageRole::User),
        Some("assistant") => Some(MessageRole::Assistant),
        Some("tool") => Some(MessageRole::Tool),
        _ => None,
    }
}
