use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use agent_core::types::{
    AuthStyle, PlatformConfig, ProtocolKind, ProviderId, Request, Response, ResponseFormat,
    RuntimeWarning,
};

use crate::anthropic_spec::AnthropicDecodeEnvelope;
use crate::error::{AdapterError, AdapterErrorKind, AdapterOperation};
use crate::openai_spec::OpenAiDecodeEnvelope;
use crate::platform::anthropic::translator::AnthropicTranslator;
use crate::platform::openai::translator::OpenAiTranslator;
use crate::platform::openrouter::translator::OpenRouterTranslator;
use crate::translator_contract::ProtocolTranslator;

pub trait ProviderAdapter: Sync + std::fmt::Debug {
    fn id(&self) -> ProviderId;
    fn default_base_url(&self) -> &'static str;
    fn endpoint_path(&self) -> &'static str;
    fn platform_config(&self, base_url: String) -> Result<PlatformConfig, AdapterError>;
    fn encode_request(&self, req: &Request) -> Result<EncodedRequest, AdapterError>;
    fn decode_response(
        &self,
        body: &Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncodedRequest {
    pub body: Value,
    pub warnings: Vec<RuntimeWarning>,
}

const OPENAI_BASE_URL: &str = "https://api.openai.com";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api";

const OPENAI_ENDPOINT_PATH: &str = "/v1/responses";
const ANTHROPIC_ENDPOINT_PATH: &str = "/v1/messages";
const OPENROUTER_ENDPOINT_PATH: &str = "/v1/chat/completions";

#[derive(Debug, Clone, Copy)]
pub struct OpenAiAdapter;

#[derive(Debug, Clone, Copy)]
pub struct AnthropicAdapter;

#[derive(Debug, Clone, Copy)]
pub struct OpenRouterAdapter;

static OPENAI_ADAPTER: OpenAiAdapter = OpenAiAdapter;
static ANTHROPIC_ADAPTER: AnthropicAdapter = AnthropicAdapter;
static OPENROUTER_ADAPTER: OpenRouterAdapter = OpenRouterAdapter;

pub fn adapter_for(id: ProviderId) -> &'static dyn ProviderAdapter {
    match id {
        ProviderId::OpenAi => &OPENAI_ADAPTER,
        ProviderId::Anthropic => &ANTHROPIC_ADAPTER,
        ProviderId::OpenRouter => &OPENROUTER_ADAPTER,
    }
}

#[cfg(test)]
pub(crate) fn all_builtin_adapters() -> &'static [&'static dyn ProviderAdapter] {
    static ADAPTERS: [&'static dyn ProviderAdapter; 3] =
        [&OPENAI_ADAPTER, &ANTHROPIC_ADAPTER, &OPENROUTER_ADAPTER];
    &ADAPTERS
}

impl ProviderAdapter for OpenAiAdapter {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAi
    }

    fn default_base_url(&self) -> &'static str {
        OPENAI_BASE_URL
    }

    fn endpoint_path(&self) -> &'static str {
        OPENAI_ENDPOINT_PATH
    }

    fn platform_config(&self, base_url: String) -> Result<PlatformConfig, AdapterError> {
        build_platform_config(
            self.id(),
            base_url,
            ProtocolKind::OpenAI,
            AuthStyle::Bearer,
            HeaderName::from_static("x-request-id"),
            HeaderMap::new(),
        )
    }

    fn encode_request(&self, req: &Request) -> Result<EncodedRequest, AdapterError> {
        let translator = OpenAiTranslator;
        let encoded = translator.encode_request(req).map_err(AdapterError::from)?;
        Ok(EncodedRequest {
            body: encoded.body,
            warnings: encoded.warnings,
        })
    }

    fn decode_response(
        &self,
        body: &Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        let translator = OpenAiTranslator;
        let envelope = OpenAiDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        };
        translator
            .decode_request(&envelope)
            .map_err(AdapterError::from)
    }
}

impl ProviderAdapter for AnthropicAdapter {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn default_base_url(&self) -> &'static str {
        ANTHROPIC_BASE_URL
    }

    fn endpoint_path(&self) -> &'static str {
        ANTHROPIC_ENDPOINT_PATH
    }

    fn platform_config(&self, base_url: String) -> Result<PlatformConfig, AdapterError> {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );

        build_platform_config(
            self.id(),
            base_url,
            ProtocolKind::Anthropic,
            AuthStyle::ApiKeyHeader(HeaderName::from_static("x-api-key")),
            HeaderName::from_static("request-id"),
            default_headers,
        )
    }

    fn encode_request(&self, req: &Request) -> Result<EncodedRequest, AdapterError> {
        let translator = AnthropicTranslator;
        let encoded = translator.encode_request(req).map_err(AdapterError::from)?;
        Ok(EncodedRequest {
            body: encoded.body,
            warnings: encoded.warnings,
        })
    }

    fn decode_response(
        &self,
        body: &Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        let translator = AnthropicTranslator;
        let envelope = AnthropicDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        };
        translator
            .decode_request(&envelope)
            .map_err(AdapterError::from)
    }
}

impl ProviderAdapter for OpenRouterAdapter {
    fn id(&self) -> ProviderId {
        ProviderId::OpenRouter
    }

    fn default_base_url(&self) -> &'static str {
        OPENROUTER_BASE_URL
    }

    fn endpoint_path(&self) -> &'static str {
        OPENROUTER_ENDPOINT_PATH
    }

    fn platform_config(&self, base_url: String) -> Result<PlatformConfig, AdapterError> {
        build_platform_config(
            self.id(),
            base_url,
            ProtocolKind::OpenAI,
            AuthStyle::Bearer,
            HeaderName::from_static("x-request-id"),
            HeaderMap::new(),
        )
    }

    fn encode_request(&self, req: &Request) -> Result<EncodedRequest, AdapterError> {
        let translator = OpenRouterTranslator::default();
        let encoded = translator.encode_request(req).map_err(AdapterError::from)?;
        Ok(EncodedRequest {
            body: encoded.body,
            warnings: encoded.warnings,
        })
    }

    fn decode_response(
        &self,
        body: &Value,
        requested_format: &ResponseFormat,
    ) -> Result<Response, AdapterError> {
        let translator = OpenRouterTranslator::default();
        let envelope = OpenAiDecodeEnvelope {
            body: body.clone(),
            requested_response_format: requested_format.clone(),
        };
        translator
            .decode_request(&envelope)
            .map_err(AdapterError::from)
    }
}

fn build_platform_config(
    provider: ProviderId,
    base_url: String,
    protocol: ProtocolKind,
    auth_style: AuthStyle,
    request_id_header: HeaderName,
    default_headers: HeaderMap,
) -> Result<PlatformConfig, AdapterError> {
    let trimmed_base_url = base_url.trim().to_string();
    if trimmed_base_url.is_empty() {
        return Err(AdapterError::new(
            AdapterErrorKind::Validation,
            provider,
            AdapterOperation::BuildHttpRequest,
            format!("base_url is empty for provider {provider:?}"),
        ));
    }

    Ok(PlatformConfig {
        protocol,
        base_url: trimmed_base_url,
        auth_style,
        request_id_header,
        default_headers,
    })
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use serde_json::json;

    use agent_core::types::{ContentPart, Message, MessageRole, ToolChoice};

    use crate::platform::anthropic::translator::AnthropicTranslator;
    use crate::platform::openai::translator::OpenAiTranslator;
    use crate::platform::openrouter::translator::OpenRouterTranslator;

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
}
