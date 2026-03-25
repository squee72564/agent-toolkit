use agent_core::{
    AnthropicCacheControl, AnthropicOptions, AnthropicOutputConfig, AnthropicServiceTier,
    AnthropicToolChoiceOptions, ProviderKind, ProviderOptions, Response, ResponseFormat,
    TaskRequest, ToolChoice,
};
use serde_json::{Map, Value, to_value};

use crate::{
    error::{AdapterError, AdapterErrorKind, AdapterOperation, ProviderErrorInfo},
    families::anthropic::wire::decode::parse_anthropic_error_value,
    interfaces::{ProviderRefinement, ProviderStreamProjector},
    request_plan::EncodedFamilyRequest,
};

#[derive(Debug, Clone, Default, PartialEq)]
struct AnthropicNativeOptionsOverrides {
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    top_k: Option<u32>,
    stop_sequences: Vec<String>,
    metadata_user_id: Option<String>,
    metadata: Map<String, Value>,
    output_config: Option<AnthropicOutputConfig>,
    service_tier: Option<&'static str>,
    tool_choice: Option<AnthropicToolChoiceOptions>,
    inference_geo: Option<String>,
    cache_control: Option<AnthropicCacheControl>,
}

impl AnthropicNativeOptionsOverrides {
    fn from_options(provider_options: Option<&ProviderOptions>) -> Result<Self, AdapterError> {
        let mut overrides = Self::default();

        if let Some(provider_options) = provider_options {
            let ProviderOptions::Anthropic(AnthropicOptions {
                temperature,
                top_p,
                max_tokens,
                top_k,
                stop_sequences,
                metadata_user_id,
                output_config,
                service_tier,
                tool_choice,
                inference_geo,
                cache_control,
                metadata,
                ..
            }) = provider_options
            else {
                return Err(AdapterError::new(
                    AdapterErrorKind::Validation,
                    ProviderKind::Anthropic,
                    AdapterOperation::PlanRequest,
                    format!(
                        "Anthropic refinement layer received mismatched provider native options for {:?}",
                        provider_options.provider_kind()
                    ),
                ));
            };

            overrides.temperature = *temperature;
            overrides.top_p = *top_p;
            overrides.max_tokens = *max_tokens;
            overrides.top_k = *top_k;
            overrides.stop_sequences = stop_sequences.clone();
            overrides.metadata_user_id = metadata_user_id.clone();
            overrides.metadata = metadata
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                .collect();
            overrides.output_config = output_config.clone();
            overrides.service_tier = service_tier.as_ref().map(service_tier_name);
            overrides.tool_choice = tool_choice.clone();
            overrides.inference_geo = inference_geo.clone();
            overrides.cache_control = cache_control.clone();
        }

        overrides.validate()?;

        Ok(overrides)
    }

    fn apply(
        &self,
        task: &TaskRequest,
        request: &mut EncodedFamilyRequest,
    ) -> Result<(), AdapterError> {
        let Some(body) = request.body.as_object_mut() else {
            return Err(AdapterError::new(
                AdapterErrorKind::ProtocolViolation,
                ProviderKind::Anthropic,
                AdapterOperation::PlanRequest,
                "Anthropic family request body must be an object",
            ));
        };

        validate_thinking_budget(body, self.max_tokens)?;

        if let Some(temperature) = self.temperature {
            insert_f32(body, "temperature", temperature)?;
        }
        if let Some(top_p) = self.top_p {
            insert_f32(body, "top_p", top_p)?;
        }
        if let Some(max_tokens) = self.max_tokens {
            body.insert("max_tokens".to_string(), Value::from(max_tokens));
        }
        if let Some(top_k) = self.top_k {
            body.insert("top_k".to_string(), Value::from(top_k));
        }
        if !self.stop_sequences.is_empty() {
            body.insert(
                "stop_sequences".to_string(),
                Value::Array(
                    self.stop_sequences
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        if let Some(metadata) = merge_metadata(&self.metadata, self.metadata_user_id.as_deref()) {
            body.insert("metadata".to_string(), Value::Object(metadata));
        }
        if let Some(service_tier) = self.service_tier {
            body.insert(
                "service_tier".to_string(),
                Value::String(service_tier.to_string()),
            );
        }
        if let Some(inference_geo) = self.inference_geo.as_ref() {
            body.insert(
                "inference_geo".to_string(),
                Value::String(inference_geo.clone()),
            );
        }
        if let Some(cache_control) = self.cache_control.as_ref() {
            body.insert(
                "cache_control".to_string(),
                serialize_cache_control(cache_control)?,
            );
        }
        if let Some(output_config) = self.output_config.as_ref() {
            merge_output_config(body, output_config)?;
        }
        if let Some(tool_choice) = self.tool_choice.as_ref() {
            // TODO: Revisit ownership if semantic tool_choice is moved out of TaskRequest.
            merge_tool_choice(task, body, tool_choice)?;
        }

        Ok(())
    }

    fn validate(&self) -> Result<(), AdapterError> {
        validate_unit_interval("temperature", self.temperature)?;
        validate_unit_interval("top_p", self.top_p)?;

        if self.max_tokens == Some(0) {
            return Err(validation_error(
                "Anthropic max_tokens must be greater than or equal to 1",
            ));
        }

        for stop_sequence in &self.stop_sequences {
            if stop_sequence.trim().is_empty() {
                return Err(validation_error(
                    "Anthropic stop_sequences entries must not be blank",
                ));
            }
        }

        if self.metadata_user_id.as_ref().map_or(0, String::len) > 256 {
            return Err(validation_error(
                "Anthropic metadata.user_id must be at most 256 characters",
            ));
        }

        if let Some(output_config) = self.output_config.as_ref()
            && output_config.format.is_some()
        {
            return Err(validation_error(
                "Anthropic output_config.format is owned by semantic response_format",
            ));
        }

        Ok(())
    }
}

fn service_tier_name(value: &AnthropicServiceTier) -> &'static str {
    match value {
        AnthropicServiceTier::Auto => "auto",
        AnthropicServiceTier::StandardOnly => "standard_only",
    }
}

fn insert_f32(body: &mut Map<String, Value>, key: &str, value: f32) -> Result<(), AdapterError> {
    let number = serde_json::Number::from_f64(f64::from(value))
        .ok_or_else(|| validation_error(format!("Anthropic {key} must be finite")))?;
    body.insert(key.to_string(), Value::Number(number));
    Ok(())
}

fn validate_thinking_budget(
    body: &Map<String, Value>,
    max_tokens: Option<u32>,
) -> Result<(), AdapterError> {
    let Some(thinking) = body.get("thinking") else {
        return Ok(());
    };

    let Some(thinking) = thinking.as_object() else {
        return Err(protocol_error(
            "Anthropic family request body must contain an object thinking field",
        ));
    };

    if thinking.get("type").and_then(Value::as_str) != Some("enabled") {
        return Ok(());
    }

    let Some(max_tokens) = max_tokens else {
        return Err(validation_error(
            "Anthropic enabled thinking requires max_tokens in provider options",
        ));
    };

    let Some(budget_tokens) = thinking.get("budget_tokens").and_then(Value::as_u64) else {
        return Err(protocol_error(
            "Anthropic enabled thinking payload must include integer budget_tokens",
        ));
    };

    if budget_tokens >= u64::from(max_tokens) {
        return Err(validation_error(
            "Anthropic thinking.budget_tokens must be less than max_tokens",
        ));
    }

    Ok(())
}

fn validate_unit_interval(field: &str, value: Option<f32>) -> Result<(), AdapterError> {
    let Some(value) = value else {
        return Ok(());
    };

    if !(0.0..=1.0).contains(&value) {
        return Err(validation_error(format!(
            "Anthropic {field} must be between 0.0 and 1.0",
        )));
    }

    Ok(())
}

fn merge_output_config(
    body: &mut Map<String, Value>,
    output_config: &AnthropicOutputConfig,
) -> Result<(), AdapterError> {
    let output_config = to_value(output_config).map_err(|error| {
        AdapterError::with_source(
            AdapterErrorKind::Encode,
            ProviderKind::Anthropic,
            AdapterOperation::PlanRequest,
            "failed to serialize Anthropic output_config",
            error,
        )
    })?;
    let Some(output_config) = output_config.as_object() else {
        return Err(protocol_error(
            "Anthropic provider-native output_config must serialize as an object",
        ));
    };

    match body.get_mut("output_config") {
        Some(Value::Object(existing)) => {
            for (key, value) in output_config {
                existing.insert(key.clone(), value.clone());
            }
        }
        Some(_) => {
            return Err(protocol_error(
                "Anthropic family request body must contain an object output_config field",
            ));
        }
        None => {
            body.insert(
                "output_config".to_string(),
                Value::Object(output_config.clone()),
            );
        }
    }

    Ok(())
}

fn merge_tool_choice(
    task: &TaskRequest,
    body: &mut Map<String, Value>,
    provider_tool_choice: &AnthropicToolChoiceOptions,
) -> Result<(), AdapterError> {
    let Some(tool_choice) = body.get_mut("tool_choice") else {
        return Err(protocol_error(
            "Anthropic family request body must contain a tool_choice field",
        ));
    };

    let Some(tool_choice) = tool_choice.as_object_mut() else {
        return Err(protocol_error(
            "Anthropic family request body must contain an object tool_choice field",
        ));
    };

    let override_spec = provider_tool_choice_override(provider_tool_choice);
    validate_tool_choice_compatibility(task, tool_choice, override_spec)?;

    if let Some(disable_parallel_tool_use) = override_spec.disable_parallel_tool_use {
        tool_choice.insert(
            "disable_parallel_tool_use".to_string(),
            Value::Bool(disable_parallel_tool_use),
        );
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ProviderToolChoiceOverride<'a> {
    type_name: &'static str,
    name: Option<&'a str>,
    disable_parallel_tool_use: Option<bool>,
}

fn provider_tool_choice_override(
    value: &AnthropicToolChoiceOptions,
) -> ProviderToolChoiceOverride<'_> {
    match value {
        AnthropicToolChoiceOptions::Auto {
            disable_parallel_tool_use,
        } => ProviderToolChoiceOverride {
            type_name: "auto",
            name: None,
            disable_parallel_tool_use: *disable_parallel_tool_use,
        },
        AnthropicToolChoiceOptions::Any {
            disable_parallel_tool_use,
        } => ProviderToolChoiceOverride {
            type_name: "any",
            name: None,
            disable_parallel_tool_use: *disable_parallel_tool_use,
        },
        AnthropicToolChoiceOptions::Tool {
            disable_parallel_tool_use,
            name,
        } => ProviderToolChoiceOverride {
            type_name: "tool",
            name: Some(name.as_str()),
            disable_parallel_tool_use: *disable_parallel_tool_use,
        },
        AnthropicToolChoiceOptions::None => ProviderToolChoiceOverride {
            type_name: "none",
            name: None,
            disable_parallel_tool_use: None,
        },
    }
}

fn semantic_tool_choice_name(task: &TaskRequest) -> Option<&str> {
    match &task.tool_choice {
        ToolChoice::Specific { name } => Some(name.as_str()),
        _ => None,
    }
}

fn semantic_tool_choice_type(task: &TaskRequest) -> &'static str {
    match task.tool_choice {
        ToolChoice::None => "none",
        ToolChoice::Auto => "auto",
        ToolChoice::Required => "any",
        ToolChoice::Specific { .. } => "tool",
    }
}

fn validate_tool_choice_compatibility(
    task: &TaskRequest,
    tool_choice: &Map<String, Value>,
    override_spec: ProviderToolChoiceOverride<'_>,
) -> Result<(), AdapterError> {
    let Some(encoded_type) = tool_choice.get("type").and_then(Value::as_str) else {
        return Err(protocol_error(
            "Anthropic family request tool_choice must include a string type",
        ));
    };

    let semantic_type = semantic_tool_choice_type(task);
    if encoded_type != semantic_type || override_spec.type_name != semantic_type {
        return Err(validation_error(
            "Anthropic provider tool_choice must match semantic task tool_choice type",
        ));
    }

    if let Some(override_name) = override_spec.name {
        let Some(encoded_name) = tool_choice.get("name").and_then(Value::as_str) else {
            return Err(protocol_error(
                "Anthropic family request tool_choice type=tool must include a name",
            ));
        };
        if semantic_tool_choice_name(task) != Some(override_name) || encoded_name != override_name {
            return Err(validation_error(
                "Anthropic provider tool_choice.name must match semantic task tool_choice name",
            ));
        }
    }

    Ok(())
}

fn serialize_cache_control(cache_control: &AnthropicCacheControl) -> Result<Value, AdapterError> {
    to_value(cache_control).map_err(|error| {
        AdapterError::with_source(
            AdapterErrorKind::Encode,
            ProviderKind::Anthropic,
            AdapterOperation::PlanRequest,
            "failed to serialize Anthropic cache_control",
            error,
        )
    })
}

fn merge_metadata(
    metadata: &Map<String, Value>,
    metadata_user_id: Option<&str>,
) -> Option<Map<String, Value>> {
    let mut merged = metadata.clone();
    if let Some(metadata_user_id) = metadata_user_id {
        merged.insert(
            "user_id".to_string(),
            Value::String(metadata_user_id.to_string()),
        );
    }

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn validation_error(message: impl Into<String>) -> AdapterError {
    AdapterError::new(
        AdapterErrorKind::Validation,
        ProviderKind::Anthropic,
        AdapterOperation::PlanRequest,
        message,
    )
}

fn protocol_error(message: impl Into<String>) -> AdapterError {
    AdapterError::new(
        AdapterErrorKind::ProtocolViolation,
        ProviderKind::Anthropic,
        AdapterOperation::PlanRequest,
        message,
    )
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnthropicRefinement;

impl ProviderRefinement for AnthropicRefinement {
    fn refine_request(
        &self,
        task: &TaskRequest,
        _model: &str,
        request: &mut EncodedFamilyRequest,
        provider_options: Option<&ProviderOptions>,
    ) -> Result<(), AdapterError> {
        AnthropicNativeOptionsOverrides::from_options(provider_options)?.apply(task, request)
    }

    fn decode_provider_error(&self, body: &Value) -> Option<ProviderErrorInfo> {
        let root = body.as_object()?;
        let envelope = parse_anthropic_error_value(root)?;
        Some(ProviderErrorInfo {
            provider_code: envelope.error_type,
            message: None,
            kind: None,
        })
    }

    fn decode_response_override(
        &self,
        _body: Value,
        _requested_format: &ResponseFormat,
    ) -> Option<Result<Response, AdapterError>> {
        None
    }

    fn create_stream_projector_override(&self) -> Option<Box<dyn ProviderStreamProjector>> {
        None
    }
}
