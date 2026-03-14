use serde_json::json;

use crate::openai_family::types::{
    OpenAiFunctionToolDefinition, OpenAiResponsesBody, OpenAiResponsesStreamEvent,
    OpenAiTextFormat, OpenAiToolType, StructuredOutputFormat,
};
use crate::test_fixtures::{load_decoded_success_fixture, load_streaming_success_fixture};

#[test]
fn structured_output_defaults_additional_properties_false() {
    let format = StructuredOutputFormat {
        name: "result".to_string(),
        description: None,
        schema: Some(json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" }
            },
            "required": ["ok"]
        })),
        strict: Some(true),
    }
    .with_default_additional_properties_false();

    assert_eq!(
        format
            .schema
            .as_ref()
            .and_then(|schema| schema.get("additionalProperties")),
        Some(&json!(false))
    );
}

#[test]
fn shared_text_format_serializes_responses_api_shape() {
    let format = OpenAiTextFormat::json_schema(StructuredOutputFormat {
        name: "result".to_string(),
        description: Some("Structured result".to_string()),
        schema: Some(json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" }
            },
            "required": ["ok"]
        })),
        strict: Some(true),
    });

    let value = serde_json::to_value(format).expect("format should serialize");
    assert_eq!(value["type"], json!("json_schema"));
    assert_eq!(value["name"], json!("result"));
    assert_eq!(value["strict"], json!(true));
    assert_eq!(value["schema"]["additionalProperties"], json!(false));
}

#[test]
fn shared_tool_definition_serializes_responses_api_shape() {
    let tool = OpenAiFunctionToolDefinition {
        tool_type: OpenAiToolType::Function,
        name: "get_weather".to_string(),
        description: Some("Get current weather by city".to_string()),
        parameters: json!({
            "type": "object",
            "properties": {
                "city": { "type": "string" }
            },
            "required": ["city"],
            "additionalProperties": false
        }),
        strict: Some(true),
    };

    let value = serde_json::to_value(tool).expect("tool should serialize");
    assert_eq!(value["type"], json!("function"));
    assert_eq!(value["name"], json!("get_weather"));
    assert_eq!(value["strict"], json!(true));
}

#[test]
fn decoded_fixtures_deserialize_shared_responses_body() {
    let fixtures = [
        ("openai", "basic_chat", "gpt-5-mini"),
        ("openai", "tool_call", "gpt-5-mini"),
        ("openrouter", "basic_chat", "openai.gpt-5.4"),
        ("openrouter", "tool_call", "openai.gpt-5.4"),
    ];

    for (provider, scenario, model) in fixtures {
        let body = load_decoded_success_fixture(provider, scenario, model);
        let parsed: OpenAiResponsesBody =
            serde_json::from_value(body).expect("fixture should deserialize into shared body");

        assert_eq!(parsed.status.as_deref(), Some("completed"));
        assert!(
            parsed
                .model
                .as_deref()
                .is_some_and(|model| !model.is_empty())
        );
        assert!(
            parsed
                .output
                .as_ref()
                .and_then(serde_json::Value::as_array)
                .is_some_and(|output| !output.is_empty())
        );
    }
}

#[test]
fn streaming_fixtures_deserialize_shared_responses_events() {
    let fixtures = [
        ("openai", "basic_chat", "gpt-5-mini"),
        ("openai", "tool_call", "gpt-5-mini"),
        ("openrouter", "basic_chat", "openai.gpt-5.4"),
        ("openrouter", "tool_call", "openai.gpt-5.4"),
    ];

    for (provider, scenario, model) in fixtures {
        let fixture = load_streaming_success_fixture(provider, scenario, model);
        let events = fixture["stream"]["events"]
            .as_array()
            .expect("stream fixture should expose parsed events");

        let mut parsed_count = 0usize;
        for event in events {
            let Some(data_json) = event.get("data_json") else {
                continue;
            };

            let parsed: OpenAiResponsesStreamEvent = serde_json::from_value(data_json.clone())
                .expect("stream event should deserialize into shared event");
            assert!(
                parsed
                    .event_type
                    .as_deref()
                    .is_some_and(|event_type| !event_type.is_empty())
            );
            parsed_count += 1;
        }

        assert!(
            parsed_count > 0,
            "expected parsed stream events for fixture"
        );
    }
}
