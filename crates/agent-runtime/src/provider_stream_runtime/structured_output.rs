use agent_core::{ContentPart, ResponseFormat, RuntimeWarning};
use serde_json::Value;

pub(super) struct StructuredOutputPayload {
    pub(super) structured_output: Option<Value>,
    pub(super) warnings: Vec<RuntimeWarning>,
}

pub(super) fn decode_structured_output_payload(
    response_format: &ResponseFormat,
    content: &[ContentPart],
) -> StructuredOutputPayload {
    match response_format {
        ResponseFormat::Text => StructuredOutputPayload {
            structured_output: None,
            warnings: Vec::new(),
        },
        ResponseFormat::JsonObject | ResponseFormat::JsonSchema { .. } => {
            let Some(text) = content.iter().find_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            }) else {
                return StructuredOutputPayload {
                    structured_output: None,
                    warnings: Vec::new(),
                };
            };

            match serde_json::from_str::<Value>(text) {
                Ok(value) if value.is_object() => StructuredOutputPayload {
                    structured_output: Some(value),
                    warnings: Vec::new(),
                },
                Ok(_) => StructuredOutputPayload {
                    structured_output: None,
                    warnings: vec![RuntimeWarning {
                        code: "runtime.stream.structured_output_not_object".to_string(),
                        message: "streamed structured output was not a JSON object".to_string(),
                    }],
                },
                Err(error) => StructuredOutputPayload {
                    structured_output: None,
                    warnings: vec![RuntimeWarning {
                        code: "runtime.stream.structured_output_parse_failed".to_string(),
                        message: format!("failed to parse streamed structured output: {error}"),
                    }],
                },
            }
        }
    }
}
