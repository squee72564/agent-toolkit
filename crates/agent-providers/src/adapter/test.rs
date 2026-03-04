use std::collections::BTreeMap;

use reqwest::header::{HeaderName, HeaderValue};
use serde_json::json;

use agent_core::types::{
    AuthStyle, ContentPart, Message, MessageRole, ProtocolKind, ProviderId, Request,
    ResponseFormat, ToolChoice,
};

use crate::anthropic_spec::AnthropicDecodeEnvelope;
use crate::openai_spec::OpenAiDecodeEnvelope;
use crate::platform::anthropic::translator::AnthropicTranslator;
use crate::platform::openai::translator::OpenAiTranslator;
use crate::platform::openrouter::translator::OpenRouterTranslator;
use crate::translator_contract::ProtocolTranslator;

use super::*;

fn base_request() -> Request {
    Request {
        model_id: "openai/gpt-5-mini".to_string(),
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
}

#[test]
fn all_builtin_adapters_contains_all_known_providers() {
    let ids: Vec<ProviderId> = all_builtin_adapters()
        .iter()
        .map(|adapter| adapter.id())
        .collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&ProviderId::OpenAi));
    assert!(ids.contains(&ProviderId::Anthropic));
    assert!(ids.contains(&ProviderId::OpenRouter));
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
fn platform_config_rejects_empty_base_url() {
    let error = adapter_for(ProviderId::OpenAi)
        .platform_config("  ".to_string())
        .expect_err("empty base url must fail");
    assert_eq!(error.provider, ProviderId::OpenAi);
    assert_eq!(error.operation, AdapterOperation::BuildHttpRequest);
    assert_eq!(error.kind, AdapterErrorKind::Validation);
}

#[test]
fn openai_adapter_encode_decode_matches_translator() {
    let request = base_request();
    let adapter = adapter_for(ProviderId::OpenAi);

    let translated_encoded = OpenAiTranslator
        .encode_request(&request)
        .expect("translator encode should succeed");
    let adapter_encoded = adapter
        .encode_request(&request)
        .expect("adapter encode should succeed");
    assert_eq!(adapter_encoded.body, translated_encoded.body);
    assert_eq!(adapter_encoded.warnings, translated_encoded.warnings);

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
    let translated_decoded = OpenAiTranslator
        .decode_request(&OpenAiDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        })
        .expect("translator decode should succeed");
    let adapter_decoded = adapter
        .decode_response(&body, &requested_format)
        .expect("adapter decode should succeed");
    assert_eq!(adapter_decoded, translated_decoded);
}

#[test]
fn anthropic_adapter_encode_decode_matches_translator() {
    let mut request = base_request();
    request.model_id = "claude-sonnet-4-6".to_string();
    let adapter = adapter_for(ProviderId::Anthropic);

    let translated_encoded = AnthropicTranslator
        .encode_request(&request)
        .expect("translator encode should succeed");
    let adapter_encoded = adapter
        .encode_request(&request)
        .expect("adapter encode should succeed");
    assert_eq!(adapter_encoded.body, translated_encoded.body);
    assert_eq!(adapter_encoded.warnings, translated_encoded.warnings);

    let body = json!({
        "role": "assistant",
        "model": "claude-sonnet-4-6",
        "stop_reason": "end_turn",
        "content": [{"type":"text","text":"hello"}],
        "usage": {"input_tokens": 1, "output_tokens": 1}
    });
    let requested_format = ResponseFormat::Text;
    let translated_decoded = AnthropicTranslator
        .decode_request(&AnthropicDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        })
        .expect("translator decode should succeed");
    let adapter_decoded = adapter
        .decode_response(&body, &requested_format)
        .expect("adapter decode should succeed");
    assert_eq!(adapter_decoded, translated_decoded);
}

#[test]
fn openrouter_adapter_preserves_fallback_decode_warning() {
    let adapter = adapter_for(ProviderId::OpenRouter);
    let payload = json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "model": "openai/gpt-5-mini",
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "message": {
                "role": "assistant",
                "content": "hello from openrouter format"
            }
        }],
        "usage": {
            "prompt_tokens": 5,
            "completion_tokens": 6,
            "total_tokens": 11
        }
    });

    let response = adapter
        .decode_response(&payload, &ResponseFormat::Text)
        .expect("decode should succeed");
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.code == "openrouter.decode.fallback_chat_completions")
    );

    let translator_response = OpenRouterTranslator::default()
        .decode_request(&OpenAiDecodeEnvelope {
            body: payload,
            requested_response_format: ResponseFormat::Text,
        })
        .expect("translator decode should succeed");
    assert_eq!(response, translator_response);
}
