use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Value, json};

use agent_core::types::{
    AssistantOutput, AuthCredentials, ContentPart, ExecutionPlan, FinishReason, Message,
    MessageRole, NativeOptions, PlatformConfig, ProviderCapabilities, ProviderInstanceId,
    ProviderKind, ResolvedAuthContext, ResolvedProviderAttempt, ResolvedTransportOptions, Response,
    ResponseFormat, ResponseMode, TaskRequest, ToolChoice, TransportTimeoutOverrides, Usage,
};

use crate::error::{AdapterError, AdapterErrorKind, ProviderErrorInfo};
use crate::interfaces::{codec_for, refinement_for, ProviderStreamProjector};
use crate::adapter::{adapter_for, all_builtin_adapters};

pub(super) fn decode_error_with_composition_test_hook<Family, Refine>(
    family_decode: Family,
    refine_error: Refine,
) -> Option<ProviderErrorInfo>
where
    Family: FnOnce() -> Option<ProviderErrorInfo>,
    Refine: FnOnce() -> Option<ProviderErrorInfo>,
{
    let family_info = family_decode();
    let refinement_info = refine_error();

    match (family_info, refinement_info) {
        (Some(family_info), Some(refinement_info)) => {
            Some(family_info.refined_with(refinement_info))
        }
        (Some(family_info), None) => Some(family_info),
        (None, Some(refinement_info)) => Some(refinement_info),
        (None, None) => None,
    }
}

pub(super) fn create_stream_projector_with_composition_test_hook<Refine, Family>(
    refine_projector: Refine,
    family_projector: Family,
) -> Box<dyn ProviderStreamProjector>
where
    Refine: FnOnce() -> Option<Box<dyn ProviderStreamProjector>>,
    Family: FnOnce() -> Box<dyn ProviderStreamProjector>,
{
    refine_projector().unwrap_or_else(family_projector)
}

pub(super) fn decode_response_with_composition_test_hook<Override, Family, RefineError>(
    body: Value,
    requested_format: &ResponseFormat,
    refinement_override: Override,
    family_decode: Family,
    refine_family_error: RefineError,
) -> Result<Response, AdapterError>
where
    Override: FnOnce(Value, &ResponseFormat) -> Option<Result<Response, AdapterError>>,
    Family: FnOnce(Value, &ResponseFormat) -> Result<Response, AdapterError>,
    RefineError: FnOnce(AdapterError) -> AdapterError,
{
    if let Some(result) = refinement_override(body.clone(), requested_format) {
        return result;
    }

    family_decode(body, requested_format).map_err(refine_family_error)
}

pub(super) fn base_task() -> TaskRequest {
    TaskRequest {
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

pub(super) fn execution_plan(
    provider: ProviderKind,
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
    native_options: Option<NativeOptions>,
) -> ExecutionPlan {
    let adapter = adapter_for(provider);
    let instance_id = match provider {
        ProviderKind::OpenAi => ProviderInstanceId::openai_default(),
        ProviderKind::Anthropic => ProviderInstanceId::anthropic_default(),
        ProviderKind::OpenRouter => ProviderInstanceId::openrouter_default(),
        ProviderKind::GenericOpenAiCompatible => {
            ProviderInstanceId::generic_openai_compatible_default()
        }
    };
    ExecutionPlan {
        response_mode,
        task: task.clone(),
        provider_attempt: ResolvedProviderAttempt {
            instance_id,
            provider_kind: provider,
            family: adapter.descriptor().family,
            model: model.to_string(),
            capabilities: *adapter.capabilities(),
            native_options,
        },
        platform: PlatformConfig {
            protocol: adapter.descriptor().protocol.clone(),
            base_url: adapter.descriptor().default_base_url.to_string(),
            auth_style: adapter.descriptor().default_auth_style.clone(),
            request_id_header: adapter.descriptor().default_request_id_header.clone(),
            default_headers: adapter.descriptor().default_headers.clone(),
        },
        auth: ResolvedAuthContext {
            credentials: Some(AuthCredentials::Token("test-key".to_string())),
        },
        transport: ResolvedTransportOptions {
            request_id_header_override: None,
            route_extra_headers: BTreeMap::new(),
            attempt_extra_headers: BTreeMap::new(),
            timeouts: TransportTimeoutOverrides::default(),
            retry_policy: agent_core::RetryPolicy::default(),
        },
        capabilities: ProviderCapabilities {
            supports_streaming: true,
            supports_family_native_options: true,
            supports_provider_native_options: true,
        },
    }
}

pub(super) fn compose_openai_compatible_request(
    provider: ProviderKind,
    task: &TaskRequest,
    model: &str,
    response_mode: ResponseMode,
    native_options: Option<&NativeOptions>,
) -> Result<crate::request_plan::ProviderRequestPlan, crate::error::AdapterError> {
    let codec = codec_for(agent_core::ProviderFamilyId::OpenAiCompatible);
    let refinement = refinement_for(provider);
    let mut encoded = codec.encode_task(
        task,
        model,
        response_mode,
        native_options.and_then(|native| native.family.as_ref()),
    )?;
    refinement.refine_request(
        task,
        model,
        &mut encoded,
        native_options.and_then(|native| native.provider.as_ref()),
    )?;
    Ok(encoded.into())
}

#[test]
fn adapter_lookup_returns_expected_kinds() {
    assert_eq!(
        adapter_for(ProviderKind::OpenAi).kind(),
        ProviderKind::OpenAi
    );
    assert_eq!(
        adapter_for(ProviderKind::Anthropic).kind(),
        ProviderKind::Anthropic
    );
    assert_eq!(
        adapter_for(ProviderKind::OpenRouter).kind(),
        ProviderKind::OpenRouter
    );
    assert_eq!(
        adapter_for(ProviderKind::GenericOpenAiCompatible).kind(),
        ProviderKind::GenericOpenAiCompatible
    );
}

#[test]
fn all_builtin_adapters_contains_all_known_providers() {
    let ids: Vec<ProviderKind> = all_builtin_adapters()
        .iter()
        .map(|adapter| adapter.kind())
        .collect();
    assert_eq!(ids.len(), 4);
    assert!(ids.contains(&ProviderKind::OpenAi));
    assert!(ids.contains(&ProviderKind::Anthropic));
    assert!(ids.contains(&ProviderKind::OpenRouter));
    assert!(ids.contains(&ProviderKind::GenericOpenAiCompatible));
}

#[test]
fn decode_response_uses_refinement_override_before_family_fallback() {
    let format = ResponseFormat::Text;
    let call_order = Rc::new(RefCell::new(Vec::new()));
    let refinement_order = Rc::clone(&call_order);
    let family_order = Rc::clone(&call_order);

    let response = decode_response_with_composition_test_hook(
        json!({ "ignored": true }),
        &format,
        move |_body, _requested_format| {
            refinement_order.borrow_mut().push("refinement");
            Some(Ok(Response {
                output: AssistantOutput {
                    content: vec![ContentPart::Text {
                        text: "refinement".to_string(),
                    }],
                    structured_output: None,
                },
                usage: Usage::default(),
                model: "refinement-model".to_string(),
                raw_provider_response: None,
                finish_reason: FinishReason::Stop,
                warnings: Vec::new(),
            }))
        },
        move |_body, _requested_format| {
            family_order.borrow_mut().push("family");
            Ok(Response {
                output: AssistantOutput {
                    content: vec![ContentPart::Text {
                        text: "family".to_string(),
                    }],
                    structured_output: None,
                },
                usage: Usage::default(),
                model: "family-model".to_string(),
                raw_provider_response: None,
                finish_reason: FinishReason::Stop,
                warnings: Vec::new(),
            })
        },
        |error| error,
    )
    .expect("decode should succeed");

    assert_eq!(
        response.output.content,
        vec![ContentPart::Text {
            text: "refinement".to_string(),
        }]
    );
    assert_eq!(&*call_order.borrow(), &["refinement"]);
}

#[test]
fn decode_response_refines_family_error_after_family_decode_runs() {
    let format = ResponseFormat::Text;
    let call_order = Rc::new(RefCell::new(Vec::new()));
    let family_order = Rc::clone(&call_order);
    let refine_order = Rc::clone(&call_order);

    let error = decode_response_with_composition_test_hook(
        json!({ "error": true }),
        &format,
        |_body, _requested_format| None,
        move |_body, _requested_format| {
            family_order.borrow_mut().push("family");
            Err(crate::error::AdapterError::new(
                crate::error::AdapterErrorKind::Upstream,
                ProviderKind::OpenAi,
                crate::error::AdapterOperation::DecodeResponse,
                "family failure",
            ))
        },
        move |mut error| {
            refine_order.borrow_mut().push("refinement-refine");
            error.message = format!("refined: {}", error.message);
            error
        },
    )
    .expect_err("decode should fail");

    assert_eq!(error.message, "refined: family failure");
    assert_eq!(&*call_order.borrow(), &["family", "refinement-refine"]);
}

#[test]
fn decode_error_runs_family_before_refinement_merge() {
    let call_order = Rc::new(RefCell::new(Vec::new()));
    let family_order = Rc::clone(&call_order);
    let refinement_order = Rc::clone(&call_order);

    let info = decode_error_with_composition_test_hook(
        move || {
            family_order.borrow_mut().push("family");
            Some(ProviderErrorInfo {
                provider_code: Some("family_code".to_string()),
                message: Some("family message".to_string()),
                kind: Some(AdapterErrorKind::Upstream),
            })
        },
        move || {
            refinement_order.borrow_mut().push("refinement");
            Some(ProviderErrorInfo {
                provider_code: Some("refinement_code".to_string()),
                message: None,
                kind: None,
            })
        },
    )
    .expect("error info should be present");

    assert_eq!(&*call_order.borrow(), &["family", "refinement"]);
    assert_eq!(info.provider_code.as_deref(), Some("refinement_code"));
    assert_eq!(info.message.as_deref(), Some("family message"));
    assert_eq!(info.kind, Some(AdapterErrorKind::Upstream));
}

#[test]
fn decode_error_refinement_fields_win_on_collision() {
    let info = decode_error_with_composition_test_hook(
        || {
            Some(ProviderErrorInfo {
                provider_code: Some("family_code".to_string()),
                message: Some("family message".to_string()),
                kind: Some(AdapterErrorKind::Decode),
            })
        },
        || {
            Some(ProviderErrorInfo {
                provider_code: Some("refinement_code".to_string()),
                message: Some("refinement message".to_string()),
                kind: Some(AdapterErrorKind::Upstream),
            })
        },
    )
    .expect("error info should be present");

    assert_eq!(info.provider_code.as_deref(), Some("refinement_code"));
    assert_eq!(info.message.as_deref(), Some("refinement message"));
    assert_eq!(info.kind, Some(AdapterErrorKind::Upstream));
}
