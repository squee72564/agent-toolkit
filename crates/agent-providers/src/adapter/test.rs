use std::collections::BTreeMap;

use reqwest::header::{HeaderName, HeaderValue};
use serde_json::json;

use agent_core::types::{
    AuthStyle, ContentPart, Message, MessageRole, ProtocolKind, ProviderId, Request,
    ResponseFormat, ToolChoice,
};

use crate::anthropic_family::AnthropicDecodeEnvelope;
use crate::openai_family::OpenAiDecodeEnvelope;
use crate::platform::anthropic::{request as anthropic_request, response as anthropic_response};
use crate::platform::openai::{request as openai_request, response as openai_response};
use crate::platform::openrouter::{request as openrouter_request, response as openrouter_response};
use crate::request_plan::{ProviderResponseKind, ProviderTransportKind};

use super::*;

fn base_request() -> Request {
    Request {
        model_id: "openai/gpt-5-mini".to_string(),
        stream: false,
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "hello".to_string(),
            }],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

fn assert_adapter_error(
    error: AdapterError,
    provider: ProviderId,
    operation: AdapterOperation,
    kind: AdapterErrorKind,
) {
    assert_eq!(error.provider, provider);
    assert_eq!(error.operation, operation);
    assert_eq!(error.kind, kind);
    assert!(error.source_ref().is_some());
}

#[test]
fn adapter_lookup_returns_expected_ids() {
    assert_eq!(adapter_for(ProviderId::OpenAi).id(), ProviderId::OpenAi);
    assert_eq!(
        adapter_for(ProviderId::Anthropic).id(),
        ProviderId::Anthropic
    );
    assert_eq!(
        adapter_for(ProviderId::OpenRouter).id(),
        ProviderId::OpenRouter
    );
    assert_eq!(
        adapter_for(ProviderId::GenericOpenAiCompatible).id(),
        ProviderId::GenericOpenAiCompatible
    );
}

#[test]
fn all_builtin_adapters_contains_all_known_providers() {
    let ids: Vec<ProviderId> = all_builtin_adapters()
        .iter()
        .map(|adapter| adapter.id())
        .collect();
    assert_eq!(ids.len(), 4);
    assert!(ids.contains(&ProviderId::OpenAi));
    assert!(ids.contains(&ProviderId::Anthropic));
    assert!(ids.contains(&ProviderId::OpenRouter));
    assert!(ids.contains(&ProviderId::GenericOpenAiCompatible));
}

#[test]
fn openai_platform_config_is_correct() {
    let config = adapter_for(ProviderId::OpenAi)
        .platform_config("https://api.openai.com".to_string())
        .expect("platform config should succeed");
    assert_eq!(config.protocol, ProtocolKind::OpenAI);
    assert_eq!(config.auth_style, AuthStyle::Bearer);
    assert_eq!(
        config.request_id_header,
        HeaderName::from_static("x-request-id")
    );
    assert!(config.default_headers.is_empty());
}

#[test]
fn anthropic_platform_config_is_correct() {
    let config = adapter_for(ProviderId::Anthropic)
        .platform_config("https://api.anthropic.com".to_string())
        .expect("platform config should succeed");
    assert_eq!(config.protocol, ProtocolKind::Anthropic);
    assert_eq!(
        config.auth_style,
        AuthStyle::ApiKeyHeader(HeaderName::from_static("x-api-key"))
    );
    assert_eq!(
        config.request_id_header,
        HeaderName::from_static("request-id")
    );
    assert_eq!(
        config
            .default_headers
            .get(HeaderName::from_static("anthropic-version"))
            .expect("anthropic-version header must exist"),
        &HeaderValue::from_static("2023-06-01")
    );
}

#[test]
fn openrouter_platform_config_is_correct() {
    let config = adapter_for(ProviderId::OpenRouter)
        .platform_config("https://openrouter.ai/api".to_string())
        .expect("platform config should succeed");
    assert_eq!(config.protocol, ProtocolKind::OpenAI);
    assert_eq!(config.auth_style, AuthStyle::Bearer);
    assert_eq!(
        config.request_id_header,
        HeaderName::from_static("x-request-id")
    );
}

#[test]
fn generic_openai_compatible_platform_config_is_correct() {
    let config = adapter_for(ProviderId::GenericOpenAiCompatible)
        .platform_config("https://example.test/v1".to_string())
        .expect("platform config should succeed");
    assert_eq!(config.protocol, ProtocolKind::OpenAI);
    assert_eq!(config.auth_style, AuthStyle::Bearer);
    assert_eq!(
        config.request_id_header,
        HeaderName::from_static("x-request-id")
    );
}

#[test]
fn platform_config_rejects_empty_base_url() {
    let error = adapter_for(ProviderId::OpenAi)
        .platform_config("  ".to_string())
        .expect_err("empty base url must fail");
    assert_eq!(error.provider, ProviderId::OpenAi);
    assert_eq!(error.operation, AdapterOperation::BuildHttpRequest);
    assert_eq!(error.kind, AdapterErrorKind::Validation);
}

#[test]
fn platform_config_trims_base_url() {
    let config = adapter_for(ProviderId::OpenAi)
        .platform_config("  https://api.openai.com  ".to_string())
        .expect("platform config should succeed");
    assert_eq!(config.base_url, "https://api.openai.com");
}

#[test]
fn platform_config_rejects_malformed_url() {
    let error = adapter_for(ProviderId::OpenAi)
        .platform_config("not a valid url".to_string())
        .expect_err("malformed base url must fail");
    assert_eq!(error.provider, ProviderId::OpenAi);
    assert_eq!(error.operation, AdapterOperation::BuildHttpRequest);
    assert_eq!(error.kind, AdapterErrorKind::Validation);
}

#[test]
fn platform_config_rejects_non_http_scheme() {
    let error = adapter_for(ProviderId::OpenAi)
        .platform_config("ftp://api.openai.com".to_string())
        .expect_err("non-http scheme must fail");
    assert_eq!(error.provider, ProviderId::OpenAi);
    assert_eq!(error.operation, AdapterOperation::BuildHttpRequest);
    assert_eq!(error.kind, AdapterErrorKind::Validation);
}

#[test]
fn default_base_url_and_endpoint_path_are_expected() {
    let openai = adapter_for(ProviderId::OpenAi);
    assert_eq!(openai.default_base_url(), "https://api.openai.com");
    assert_eq!(openai.endpoint_path(), "/v1/responses");

    let anthropic = adapter_for(ProviderId::Anthropic);
    assert_eq!(anthropic.default_base_url(), "https://api.anthropic.com");
    assert_eq!(anthropic.endpoint_path(), "/v1/messages");

    let openrouter = adapter_for(ProviderId::OpenRouter);
    assert_eq!(openrouter.default_base_url(), "https://openrouter.ai/api");
    assert_eq!(openrouter.endpoint_path(), "/v1/responses");
}

#[test]
fn openai_adapter_plan_request_and_decode_match_translator() {
    let request = base_request();
    let adapter = adapter_for(ProviderId::OpenAi);

    let translated_encoded = openai_request::plan_request(request.clone())
        .expect("request planning should succeed");
    let adapter_plan = adapter
        .plan_request(request)
        .expect("adapter planning should succeed");
    assert_eq!(adapter_plan.body, translated_encoded.body);
    assert_eq!(adapter_plan.warnings, translated_encoded.warnings);
    assert_eq!(adapter_plan.transport_kind, ProviderTransportKind::HttpJson);
    assert_eq!(adapter_plan.response_kind, ProviderResponseKind::JsonBody);

    let body = json!({
        "status": "completed",
        "model": "gpt-5-mini",
        "output": [{
            "type": "message",
            "content": [{ "type": "output_text", "text": "hello" }]
        }],
        "usage": {
            "input_tokens": 1,
            "output_tokens": 2,
            "total_tokens": 3
        }
    });
    let requested_format = ResponseFormat::Text;
    let translated_decoded = openai_response::decode_response_json(
        OpenAiDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        }
        .body,
        &requested_format,
    )
    .expect("response decode should succeed");
    let adapter_decoded = adapter
        .decode_response_json(body, &requested_format)
        .expect("adapter decode should succeed");
    assert_eq!(adapter_decoded, translated_decoded);
}

#[test]
fn anthropic_adapter_plan_request_and_decode_match_translator() {
    let mut request = base_request();
    request.model_id = "claude-sonnet-4-6".to_string();
    let adapter = adapter_for(ProviderId::Anthropic);

    let translated_encoded = anthropic_request::plan_request(request.clone())
        .expect("request planning should succeed");
    let adapter_plan = adapter
        .plan_request(request)
        .expect("adapter planning should succeed");
    assert_eq!(adapter_plan.body, translated_encoded.body);
    assert_eq!(adapter_plan.warnings, translated_encoded.warnings);
    assert_eq!(adapter_plan.transport_kind, ProviderTransportKind::HttpJson);
    assert_eq!(adapter_plan.response_kind, ProviderResponseKind::JsonBody);

    let body = json!({
        "role": "assistant",
        "model": "claude-sonnet-4-6",
        "stop_reason": "end_turn",
        "content": [{"type":"text","text":"hello"}],
        "usage": {"input_tokens": 1, "output_tokens": 1}
    });
    let requested_format = ResponseFormat::Text;
    let translated_decoded = anthropic_response::decode_response_json(
        AnthropicDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        }
        .body,
        &requested_format,
    )
    .expect("response decode should succeed");
    let adapter_decoded = adapter
        .decode_response_json(body, &requested_format)
        .expect("adapter decode should succeed");
    assert_eq!(adapter_decoded, translated_decoded);
}

#[test]
fn openrouter_adapter_matches_direct_responses_decode() {
    let adapter = adapter_for(ProviderId::OpenRouter);
    let payload = json!({
        "status": "completed",
        "model": "openai/gpt-5-mini",
        "output": [{
            "type": "message",
            "content": [{
                "type": "output_text",
                "text": "hello from openrouter format"
            }]
        }],
        "usage": {
            "input_tokens": 5,
            "output_tokens": 6,
            "total_tokens": 11
        }
    });

    let response = adapter
        .decode_response_json(payload.clone(), &ResponseFormat::Text)
        .expect("decode should succeed");
    assert!(response.warnings.is_empty());

    let direct_response = openrouter_response::decode_response_json(payload, &ResponseFormat::Text)
        .expect("response decode should succeed");
    assert_eq!(response, direct_response);
}

#[test]
fn openai_adapter_decode_error_maps_provider_operation_and_kind() {
    let adapter = adapter_for(ProviderId::OpenAi);
    let error = adapter
        .decode_response_json(json!("invalid"), &ResponseFormat::Text)
        .expect_err("decode should fail for non-object payload");

    assert_adapter_error(
        error,
        ProviderId::OpenAi,
        AdapterOperation::DecodeResponse,
        AdapterErrorKind::Decode,
    );
}

#[test]
fn anthropic_adapter_decode_error_maps_provider_operation_and_kind() {
    let adapter = adapter_for(ProviderId::Anthropic);
    let error = adapter
        .decode_response_json(
            json!({ "model": "claude-sonnet-4-6" }),
            &ResponseFormat::Text,
        )
        .expect_err("decode should fail for malformed anthropic payload");

    assert_adapter_error(
        error,
        ProviderId::Anthropic,
        AdapterOperation::DecodeResponse,
        AdapterErrorKind::Decode,
    );
}

#[test]
fn openrouter_adapter_decode_error_maps_provider_operation_and_kind() {
    let adapter = adapter_for(ProviderId::OpenRouter);
    let error = adapter
        .decode_response_json(json!("invalid"), &ResponseFormat::Text)
        .expect_err("decode should fail for non-object payload");

    assert_adapter_error(
        error,
        ProviderId::OpenRouter,
        AdapterOperation::DecodeResponse,
        AdapterErrorKind::Decode,
    );
}

#[test]
fn openai_adapter_plan_validation_error_maps_provider_operation_and_kind() {
    let adapter = adapter_for(ProviderId::OpenAi);
    let mut request = base_request();
    request.messages.clear();

    let error = adapter
        .plan_request(request)
        .expect_err("planning should fail for empty messages");

    assert_adapter_error(
        error,
        ProviderId::OpenAi,
        AdapterOperation::PlanRequest,
        AdapterErrorKind::Validation,
    );
}

#[test]
fn anthropic_adapter_plan_validation_error_maps_provider_operation_and_kind() {
    let adapter = adapter_for(ProviderId::Anthropic);
    let mut request = base_request();
    request.model_id = "claude-sonnet-4-6".to_string();
    request.temperature = Some(1.5);

    let error = adapter
        .plan_request(request)
        .expect_err("planning should fail for out-of-range temperature");

    assert_adapter_error(
        error,
        ProviderId::Anthropic,
        AdapterOperation::PlanRequest,
        AdapterErrorKind::Validation,
    );
}

#[test]
fn openrouter_adapter_plan_validation_error_maps_provider_operation_and_kind() {
    let adapter = adapter_for(ProviderId::OpenRouter);
    let mut request = base_request();
    request.messages.clear();

    let error = adapter
        .plan_request(request)
        .expect_err("planning should fail for empty messages");

    assert_adapter_error(
        error,
        ProviderId::OpenRouter,
        AdapterOperation::PlanRequest,
        AdapterErrorKind::Validation,
    );
}
