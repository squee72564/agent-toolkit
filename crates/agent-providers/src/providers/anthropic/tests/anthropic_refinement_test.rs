use crate::error::{AdapterErrorKind, AdapterOperation};
use crate::interfaces::codec_for;
use crate::interfaces::refinement_for;
use agent_core::{
    AnthropicOptions, AnthropicOutputConfig, AnthropicOutputEffort, AnthropicOutputFormat,
    AnthropicOutputFormatType, AnthropicServiceTier, AnthropicToolChoiceOptions, ContentPart,
    Message, MessageRole, OpenAiOptions, ProviderOptions, ResponseFormat, ResponseMode,
    TaskRequest, ToolChoice,
};
use serde_json::json;

const MODEL_ID: &str = "claude-sonnet-4-6";

fn base_task() -> TaskRequest {
    TaskRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::text("hello")],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
    }
}

fn plan_with_provider_options(
    task: &TaskRequest,
    provider: AnthropicOptions,
) -> Result<crate::request_plan::EncodedFamilyRequest, crate::error::AdapterError> {
    let mut encoded = codec_for(agent_core::ProviderFamilyId::Anthropic).encode_task(
        task,
        MODEL_ID,
        ResponseMode::NonStreaming,
        None,
    )?;
    refinement_for(agent_core::ProviderKind::Anthropic).refine_request(
        task,
        MODEL_ID,
        &mut encoded,
        Some(&ProviderOptions::Anthropic(provider)),
    )?;
    Ok(encoded)
}

#[test]
fn anthropic_refinement_applies_full_provider_native_matrix() {
    let task = TaskRequest {
        tool_choice: ToolChoice::Specific {
            name: "lookup_weather".to_string(),
        },
        response_format: ResponseFormat::JsonObject,
        tools: vec![agent_core::ToolDefinition {
            name: "lookup_weather".to_string(),
            description: Some("Look up weather".to_string()),
            parameters_schema: json!({
                "type": "object",
                "properties": { "city": { "type": "string" } },
                "required": ["city"]
            }),
        }],
        ..base_task()
    };

    let encoded = plan_with_provider_options(
        &task,
        AnthropicOptions {
            temperature: Some(0.4),
            top_p: Some(0.8),
            max_tokens: Some(256),
            top_k: Some(8),
            stop_sequences: vec!["END".to_string()],
            metadata_user_id: Some("user-1".to_string()),
            output_config: Some(AnthropicOutputConfig {
                effort: Some(AnthropicOutputEffort::High),
                format: None,
            }),
            service_tier: Some(AnthropicServiceTier::StandardOnly),
            tool_choice: Some(AnthropicToolChoiceOptions {
                disable_parallel_tool_use: Some(false),
            }),
            inference_geo: Some("us".to_string()),
        },
    )
    .expect("refinement should succeed");

    let temperature = encoded.body["temperature"]
        .as_f64()
        .expect("temperature should be numeric");
    assert!((temperature - 0.4).abs() < 1e-6);
    let top_p = encoded.body["top_p"]
        .as_f64()
        .expect("top_p should be numeric");
    assert!((top_p - 0.8).abs() < 1e-6);
    assert_eq!(encoded.body["max_tokens"], 256);
    assert_eq!(encoded.body["top_k"], 8);
    assert_eq!(encoded.body["stop_sequences"], json!(["END"]));
    assert_eq!(encoded.body["metadata"], json!({ "user_id": "user-1" }));
    assert_eq!(encoded.body["service_tier"], "standard_only");
    assert_eq!(encoded.body["inference_geo"], json!("us"));
    assert_eq!(
        encoded.body.pointer("/output_config/effort"),
        Some(&json!("high"))
    );
    assert_eq!(
        encoded.body.pointer("/output_config/format/type"),
        Some(&json!("json_schema"))
    );
    assert_eq!(
        encoded
            .body
            .pointer("/tool_choice/disable_parallel_tool_use"),
        Some(&json!(false))
    );
    assert_eq!(
        encoded.body.pointer("/tool_choice/type"),
        Some(&json!("tool"))
    );
}

#[test]
fn anthropic_refinement_does_not_encode_provider_controls_without_provider_options() {
    let encoded = plan_with_provider_options(&base_task(), AnthropicOptions::default())
        .expect("refinement should succeed");

    assert!(encoded.body.get("temperature").is_none());
    assert!(encoded.body.get("top_p").is_none());
    assert!(encoded.body.get("max_tokens").is_none());
    assert!(encoded.body.get("top_k").is_none());
    assert!(encoded.body.get("stop_sequences").is_none());
    assert!(encoded.body.get("metadata_user_id").is_none());
    assert!(encoded.body.get("output_config").is_none());
    assert!(encoded.body.get("service_tier").is_none());
    assert!(encoded.body.get("inference_geo").is_none());
}

#[test]
fn anthropic_refinement_rejects_invalid_temperature() {
    let error = plan_with_provider_options(
        &base_task(),
        AnthropicOptions {
            temperature: Some(1.5),
            ..Default::default()
        },
    )
    .expect_err("refinement should reject invalid temperature");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("temperature"));
}

#[test]
fn anthropic_refinement_rejects_invalid_top_p() {
    let error = plan_with_provider_options(
        &base_task(),
        AnthropicOptions {
            top_p: Some(-0.1),
            ..Default::default()
        },
    )
    .expect_err("refinement should reject invalid top_p");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("top_p"));
}

#[test]
fn anthropic_refinement_rejects_zero_max_tokens() {
    let error = plan_with_provider_options(
        &base_task(),
        AnthropicOptions {
            max_tokens: Some(0),
            ..Default::default()
        },
    )
    .expect_err("refinement should reject zero max_tokens");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("max_tokens"));
}

#[test]
fn anthropic_refinement_rejects_enabled_thinking_budget_that_meets_max_tokens() {
    let task = base_task();
    let family_options = agent_core::FamilyOptions::Anthropic(agent_core::AnthropicFamilyOptions {
        thinking: Some(agent_core::AnthropicThinking::Enabled {
            budget_tokens: agent_core::AnthropicThinkingBudget::new(1024)
                .expect("non-zero thinking budget"),
            display: None,
        }),
    });

    let mut encoded = codec_for(agent_core::ProviderFamilyId::Anthropic)
        .encode_task(
            &task,
            MODEL_ID,
            ResponseMode::NonStreaming,
            Some(&family_options),
        )
        .expect("family planning should succeed");

    let error = refinement_for(agent_core::ProviderKind::Anthropic)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::Anthropic(AnthropicOptions {
                max_tokens: Some(1024),
                ..AnthropicOptions::default()
            })),
        )
        .expect_err("refinement should reject thinking budget that meets max_tokens");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("less than max_tokens"));
}

#[test]
fn anthropic_refinement_rejects_blank_stop_sequence() {
    let error = plan_with_provider_options(
        &base_task(),
        AnthropicOptions {
            stop_sequences: vec!["   ".to_string()],
            ..Default::default()
        },
    )
    .expect_err("refinement should reject blank stop sequence");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("stop_sequences"));
}

#[test]
fn anthropic_refinement_rejects_overlong_metadata_user_id() {
    let error = plan_with_provider_options(
        &base_task(),
        AnthropicOptions {
            metadata_user_id: Some("u".repeat(257)),
            ..Default::default()
        },
    )
    .expect_err("refinement should reject overlong metadata");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("metadata.user_id"));
}

#[test]
fn anthropic_refinement_rejects_provider_owned_output_config_format() {
    let error = plan_with_provider_options(
        &base_task(),
        AnthropicOptions {
            output_config: Some(AnthropicOutputConfig {
                effort: None,
                format: Some(AnthropicOutputFormat {
                    schema: json!({ "type": "object" }),
                    format_type: AnthropicOutputFormatType::JsonSchema,
                }),
            }),
            ..Default::default()
        },
    )
    .expect_err("refinement should reject provider-owned output_config format");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("output_config.format"));
}

#[test]
fn anthropic_refinement_rejects_tool_choice_override_for_none() {
    let task = TaskRequest {
        tool_choice: ToolChoice::None,
        ..base_task()
    };
    let error = plan_with_provider_options(
        &task,
        AnthropicOptions {
            tool_choice: Some(AnthropicToolChoiceOptions {
                disable_parallel_tool_use: Some(true),
            }),
            ..Default::default()
        },
    )
    .expect_err("refinement should reject tool_choice override for none");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert!(error.message.contains("disable_parallel_tool_use"));
}

#[test]
fn anthropic_refinement_rejects_mismatched_provider_options() {
    let task = base_task();
    let mut encoded = codec_for(agent_core::ProviderFamilyId::Anthropic)
        .encode_task(&task, MODEL_ID, ResponseMode::NonStreaming, None)
        .expect("planning should succeed");

    let error = refinement_for(agent_core::ProviderKind::Anthropic)
        .refine_request(
            &task,
            MODEL_ID,
            &mut encoded,
            Some(&ProviderOptions::OpenAi(OpenAiOptions {
                metadata: std::collections::BTreeMap::new(),
                service_tier: Some("flex".to_string()),
                store: Some(false),
                ..Default::default()
            })),
        )
        .expect_err("refinement should reject mismatched options");

    assert_eq!(error.kind, AdapterErrorKind::Validation);
    assert_eq!(error.operation, AdapterOperation::PlanRequest);
    assert!(error.message.contains("mismatched provider native options"));
}

#[test]
fn anthropic_output_config_rejects_invalid_effort_during_deserialization() {
    let error = serde_json::from_value::<AnthropicOptions>(json!({
        "output_config": { "effort": "extreme" }
    }))
    .expect_err("deserialization should fail");

    assert!(error.to_string().contains("extreme"));
}

#[test]
fn anthropic_output_config_rejects_non_object_during_deserialization() {
    let error = serde_json::from_value::<AnthropicOptions>(json!({
        "output_config": "high"
    }))
    .expect_err("deserialization should fail");

    assert!(error.to_string().contains("invalid type"));
}

#[test]
fn anthropic_output_config_rejects_unknown_format_type_during_deserialization() {
    let error = serde_json::from_value::<AnthropicOptions>(json!({
        "output_config": {
            "format": {
                "type": "xml",
                "schema": { "type": "object" }
            }
        }
    }))
    .expect_err("deserialization should fail");

    assert!(error.to_string().contains("xml"));
}

#[test]
fn anthropic_inference_geo_deserializes_as_string() {
    let options = serde_json::from_value::<AnthropicOptions>(json!({
        "inference_geo": "us"
    }))
    .expect("deserialization should succeed");

    assert_eq!(options.inference_geo.as_deref(), Some("us"));
}

#[test]
fn anthropic_inference_geo_rejects_object_during_deserialization() {
    let error = serde_json::from_value::<AnthropicOptions>(json!({
        "inference_geo": { "region": "us" }
    }))
    .expect_err("deserialization should fail");

    assert!(error.to_string().contains("string"));
}
