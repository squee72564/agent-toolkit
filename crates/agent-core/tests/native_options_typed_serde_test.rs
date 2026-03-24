use agent_core::{AnthropicOptions, OpenAiCompatibleOptions, OpenRouterOptions, OpenRouterPlugin};
use serde_json::json;

#[test]
fn openai_compatible_reasoning_round_trips() {
    let options = serde_json::from_value::<OpenAiCompatibleOptions>(json!({
        "reasoning": {
            "effort": "xhigh",
            "summary": "detailed"
        }
    }))
    .expect("deserialize reasoning");

    let serialized = serde_json::to_value(&options).expect("serialize reasoning");
    assert_eq!(
        serialized,
        json!({
            "reasoning": {
                "effort": "xhigh",
                "summary": "detailed"
            }
        })
    );
}

#[test]
fn anthropic_output_config_round_trips() {
    let options = serde_json::from_value::<AnthropicOptions>(json!({
        "output_config": {
            "effort": "high",
            "format": {
                "type": "json_schema",
                "schema": { "type": "object" }
            }
        },
        "inference_geo": "us"
    }))
    .expect("deserialize anthropic options");

    let serialized = serde_json::to_value(&options).expect("serialize anthropic options");
    assert_eq!(
        serialized,
        json!({
            "output_config": {
                "effort": "high",
                "format": {
                    "type": "json_schema",
                    "schema": { "type": "object" }
                }
            },
            "inference_geo": "us"
        })
    );
}

#[test]
fn anthropic_output_config_rejects_unknown_fields() {
    let error = serde_json::from_value::<AnthropicOptions>(json!({
        "output_config": {
            "effort": "high",
            "unknown": true
        }
    }))
    .expect_err("unknown field should fail");

    assert!(error.to_string().contains("unknown"));
}

#[test]
fn openrouter_plugin_and_trace_round_trip() {
    let options = serde_json::from_value::<OpenRouterOptions>(json!({
        "plugins": [
            {
                "id": "web",
                "enabled": true,
                "engine": "exa",
                "include_domains": ["example.com"]
            },
            {
                "id": "file-parser",
                "pdf": {
                    "engine": "mistral-ocr"
                }
            }
        ],
        "trace": {
            "trace_id": "trace-1",
            "span_name": "span-1"
        },
        "image_config": {
            "size": "1024x1024",
            "guidance": 3.5
        }
    }))
    .expect("deserialize openrouter options");

    assert!(matches!(options.plugins[0], OpenRouterPlugin::Web(_)));
    let serialized = serde_json::to_value(&options).expect("serialize openrouter options");
    assert_eq!(
        serialized,
        json!({
            "plugins": [
                {
                    "id": "web",
                    "enabled": true,
                    "engine": "exa",
                    "include_domains": ["example.com"]
                },
                {
                    "id": "file-parser",
                    "pdf": {
                        "engine": "mistral-ocr"
                    }
                }
            ],
            "trace": {
                "trace_id": "trace-1",
                "span_name": "span-1"
            },
            "image_config": {
                "size": "1024x1024",
                "guidance": 3.5
            }
        })
    );
}

#[test]
fn openrouter_plugins_reject_unknown_plugin_ids() {
    let error = serde_json::from_value::<OpenRouterOptions>(json!({
        "plugins": [
            { "id": "mystery-plugin" }
        ]
    }))
    .expect_err("unknown plugin id should fail");

    assert!(error.to_string().contains("mystery-plugin"));
}
