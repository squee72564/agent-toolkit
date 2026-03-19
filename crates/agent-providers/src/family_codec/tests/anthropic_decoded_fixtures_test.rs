use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::{Value, json};

use crate::anthropic_family::AnthropicDecodeEnvelope;
use crate::fixture_tests::{
    choose_valid_success_fixture, list_decoded_error_fixture_models,
    list_decoded_error_fixture_relpaths, list_decoded_fixture_models,
    load_decoded_error_fixture_body, load_decoded_success_fixture,
    validate_decoded_error_fixture_shape,
};
use crate::adapter::adapter_for;
use agent_core::ProviderKind;
use agent_core::types::{ContentPart, FinishReason, Response, ResponseFormat};

const PROVIDER: &str = "anthropic";
const SUCCESS_SCENARIOS: [&str; 3] = ["basic_chat", "tool_call", "tool_call_reasoning"];
const SMOKE_MODELS_ANTHROPIC: [&str; 2] = ["claude-sonnet-4-6", "claude-sonnet-4-5-20250929"];
const SMOKE_ERROR_FIXTURES: [(&str, &str); 4] = [
    ("invalid_auth", "claude-sonnet-4-5-20250929"),
    ("invalid_model", "this-model-does-not-exist"),
    ("invalid_request_schema", "claude-sonnet-4-5-20250929"),
    ("invalid_tool_payload", "claude-sonnet-4-5-20250929"),
];

#[test]
fn fixture_smoke_anthropic_basic_chat() -> Result<(), String> {
    run_success_smoke_scenario("basic_chat", &SMOKE_MODELS_ANTHROPIC)
}

fn decode_response_json(
    body: Value,
    requested_response_format: &ResponseFormat,
) -> Result<Response, crate::error::AdapterError> {
    adapter_for(ProviderKind::Anthropic).decode_response_json(body, requested_response_format)
}

#[test]
fn fixture_smoke_anthropic_tool_call() -> Result<(), String> {
    run_success_smoke_scenario("tool_call", &SMOKE_MODELS_ANTHROPIC)
}

#[test]
fn fixture_smoke_anthropic_tool_call_reasoning() -> Result<(), String> {
    run_success_smoke_scenario("tool_call_reasoning", &SMOKE_MODELS_ANTHROPIC)
}

#[test]
fn fixture_smoke_anthropic_errors() -> Result<(), String> {
    for (scenario, preferred_model) in SMOKE_ERROR_FIXTURES {
        let chosen_model = choose_error_fixture_model_for_upstream(scenario, preferred_model)?;

        let body = load_decoded_error_fixture_body(PROVIDER, scenario, &chosen_model);
        let payload = AnthropicDecodeEnvelope {
            body,
            requested_response_format: ResponseFormat::Text,
        };
        let error = decode_response_json(payload.body, &payload.requested_response_format)
            .expect_err("expected upstream decode error for error fixture");
        assert_anthropic_upstream_error(error, scenario, &chosen_model)?;
    }
    Ok(())
}

#[test]
#[ignore]
fn fixture_full_anthropic_success_sweep() -> Result<(), String> {
    for scenario in SUCCESS_SCENARIOS {
        let models = list_decoded_fixture_models(PROVIDER, scenario);
        if models.is_empty() {
            return Err(format!(
                "expected at least one fixture model for scenario {scenario}"
            ));
        }
        for model in models {
            let body = load_decoded_success_fixture(PROVIDER, scenario, &model);
            validate_success_fixture_body(&body, scenario, &model).map_err(|reason| {
                format!("invalid success fixture {scenario}/{model}: {reason}")
            })?;
        }
    }
    Ok(())
}

#[test]
#[ignore]
fn fixture_full_anthropic_errors_sweep() -> Result<(), String> {
    let relpaths = list_decoded_error_fixture_relpaths(PROVIDER);
    if relpaths.is_empty() {
        return Err(format!(
            "expected at least one error fixture relpath for provider {PROVIDER}"
        ));
    }
    for relpath in relpaths {
        let (scenario, model) = parse_error_relpath(&relpath)?;
        validate_decoded_error_fixture_shape(PROVIDER, scenario, model).map_err(|reason| {
            format!("invalid error fixture wrapper {scenario}/{model}: {reason}")
        })?;

        let body = catch_unwind(AssertUnwindSafe(|| {
            load_decoded_error_fixture_body(PROVIDER, scenario, model)
        }))
        .map_err(|payload| {
            format!(
                "failed to load error fixture body {scenario}/{model}: {}",
                panic_payload_to_string(payload)
            )
        })?;
        if !has_top_level_error_object(&body) {
            return Err(format!(
                "error fixture missing top-level error object: {scenario}/{model}"
            ));
        }

        let payload = AnthropicDecodeEnvelope {
            body,
            requested_response_format: ResponseFormat::Text,
        };
        let error = decode_response_json(payload.body, &payload.requested_response_format)
            .expect_err("expected upstream decode error for error fixture");
        assert_anthropic_upstream_error(error, scenario, model)?;
    }
    Ok(())
}

fn run_success_smoke_scenario(scenario: &str, preferred_models: &[&str]) -> Result<(), String> {
    for preferred_model in preferred_models {
        let selected = choose_valid_success_fixture(
            PROVIDER,
            scenario,
            preferred_model,
            |candidate_model, body| validate_success_fixture_body(body, scenario, candidate_model),
        );
        if selected.swapped
            && let Some(reason) = &selected.preferred_rejection_reason
        {
            eprintln!(
                "fixture swap reason: provider={PROVIDER} scenario={scenario} requested={} chosen={} reason={reason}",
                selected.requested_model, selected.chosen_model
            );
        }
        validate_success_fixture_body(&selected.body, scenario, &selected.chosen_model).map_err(
            |reason| {
                format!(
                    "selected fixture failed validation {scenario}/{}: {reason}",
                    selected.chosen_model
                )
            },
        )?;
    }
    Ok(())
}

fn choose_error_fixture_model_for_upstream(
    scenario: &str,
    preferred_model: &str,
) -> Result<String, String> {
    let mut models = catch_unwind(AssertUnwindSafe(|| {
        list_decoded_error_fixture_models(PROVIDER, scenario)
    }))
    .map_err(|payload| {
        format!(
            "failed to list error fixture models for provider={PROVIDER} scenario={scenario}: {}",
            panic_payload_to_string(payload)
        )
    })?;
    if let Some(pos) = models.iter().position(|model| model == preferred_model) {
        let preferred = models.remove(pos);
        models.insert(0, preferred);
    }

    let mut rejected = Vec::new();
    for model in models {
        let wrapper_shape = catch_unwind(AssertUnwindSafe(|| {
            validate_decoded_error_fixture_shape(PROVIDER, scenario, &model)
        }));
        match wrapper_shape {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => {
                rejected.push(format!("{model}: invalid wrapper shape: {reason}"));
                continue;
            }
            Err(payload) => {
                rejected.push(format!(
                    "{model}: wrapper shape validation panicked: {}",
                    panic_payload_to_string(payload)
                ));
                continue;
            }
        }

        let body = match catch_unwind(AssertUnwindSafe(|| {
            load_decoded_error_fixture_body(PROVIDER, scenario, &model)
        })) {
            Ok(body) => body,
            Err(payload) => {
                rejected.push(format!(
                    "{model}: failed to load response.body: {}",
                    panic_payload_to_string(payload)
                ));
                continue;
            }
        };
        if has_top_level_error_object(&body) {
            if model != preferred_model {
                eprintln!(
                    "error fixture swap: provider={PROVIDER} scenario={scenario} requested={preferred_model} chosen={model}"
                );
            }
            return Ok(model);
        }
        rejected.push(format!(
            "{model}: response.body missing top-level anthropic error object"
        ));
    }

    Err(format!(
        "no valid upstream error fixture available for provider={PROVIDER} scenario={scenario} preferred={preferred_model}; rejected=[{}]",
        rejected.join("; ")
    ))
}

fn validate_success_fixture_body(body: &Value, scenario: &str, _model: &str) -> Result<(), String> {
    let payload = AnthropicDecodeEnvelope {
        body: body.clone(),
        requested_response_format: ResponseFormat::Text,
    };
    let response = decode_response_json(payload.body, &payload.requested_response_format)
        .map_err(|err| format!("decode failed: {err}"))?;
    assert_success_invariants(&response, scenario)
}

fn assert_success_invariants(response: &Response, scenario: &str) -> Result<(), String> {
    if response.model.trim().is_empty() {
        return Err("decoded response model is empty".to_string());
    }
    if response.usage.input_tokens.is_none()
        && response.usage.output_tokens.is_none()
        && response.usage.total_tokens.is_none()
    {
        return Err("missing all usage token fields".to_string());
    }

    match scenario {
        "basic_chat" => {
            if !has_non_empty_text(response) {
                return Err("missing non-empty text output".to_string());
            }
            if response.finish_reason == FinishReason::ToolCalls {
                return Err("finish_reason must not be ToolCalls".to_string());
            }
        }
        "tool_call" => {
            if !has_tool_call(response) {
                return Err("missing decoded tool call content part".to_string());
            }
            if response.finish_reason != FinishReason::ToolCalls {
                return Err("finish_reason must be ToolCalls".to_string());
            }
        }
        "tool_call_reasoning" => {
            if !has_non_empty_text(response) {
                return Err("missing non-empty text output".to_string());
            }
            if response.finish_reason == FinishReason::ToolCalls {
                return Err("finish_reason must not be ToolCalls".to_string());
            }
        }
        other => return Err(format!("unexpected scenario: {other}")),
    }

    Ok(())
}

fn has_non_empty_text(response: &Response) -> bool {
    response.output.content.iter().any(|part| match part {
        ContentPart::Text { text } => !text.trim().is_empty(),
        _ => false,
    })
}

fn has_tool_call(response: &Response) -> bool {
    response
        .output
        .content
        .iter()
        .any(|part| matches!(part, ContentPart::ToolCall { .. }))
}

fn assert_anthropic_upstream_error(
    error: crate::error::AdapterError,
    scenario: &str,
    model: &str,
) -> Result<(), String> {
    if error.kind != crate::error::AdapterErrorKind::Upstream {
        return Err(format!(
            "expected upstream adapter error for fixture {scenario}/{model}, got kind={:?} message={}",
            error.kind, error.message
        ));
    }
    if error.message.trim().is_empty() {
        return Err(format!(
            "expected non-empty upstream message for {scenario}/{model}"
        ));
    }
    if !error.message.contains("anthropic error:") {
        return Err(format!(
            "expected provider context in upstream message for {scenario}/{model}: {}",
            error.message
        ));
    }
    Ok(())
}

fn has_top_level_error_object(body: &Value) -> bool {
    body.get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == "error")
        && body.get("error").is_some_and(Value::is_object)
}

fn parse_error_relpath(relpath: &str) -> Result<(&str, &str), String> {
    let mut parts = relpath.split('/');
    let prefix = parts.next();
    let scenario = parts.next();
    let file = parts.next();
    let extra = parts.next();

    if prefix != Some("errors") {
        return Err(format!("unexpected error relpath prefix: {relpath}"));
    }
    if extra.is_some() {
        return Err(format!("unexpected error relpath shape: {relpath}"));
    }

    let scenario =
        scenario.ok_or_else(|| format!("missing error scenario in relpath: {relpath}"))?;
    if scenario.trim().is_empty() {
        return Err(format!("empty error scenario in relpath: {relpath}"));
    }

    let file = file.ok_or_else(|| format!("missing error file in relpath: {relpath}"))?;
    let model = file
        .strip_suffix(".json")
        .ok_or_else(|| format!("error relpath does not end with .json: {relpath}"))?;
    if model.trim().is_empty() {
        return Err(format!("empty error model in relpath: {relpath}"));
    }

    Ok((scenario, model))
}

fn panic_payload_to_string(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    "unknown panic payload".to_string()
}

#[test]
fn parse_error_relpath_accepts_valid_relpath() {
    let parsed = parse_error_relpath("errors/invalid_auth/claude-sonnet-4-5-20250929.json");
    assert_eq!(parsed, Ok(("invalid_auth", "claude-sonnet-4-5-20250929")));
}

#[test]
fn parse_error_relpath_rejects_invalid_prefix() {
    let error = parse_error_relpath("not-errors/invalid_auth/model.json")
        .expect_err("expected invalid prefix");
    assert!(error.contains("unexpected error relpath prefix"));
}

#[test]
fn parse_error_relpath_rejects_missing_or_empty_segments() {
    let missing_scenario =
        parse_error_relpath("errors//model.json").expect_err("expected missing or empty scenario");
    assert!(
        missing_scenario.contains("empty error scenario")
            || missing_scenario.contains("missing error scenario")
    );

    let missing_file =
        parse_error_relpath("errors/invalid_auth").expect_err("expected missing error file");
    assert!(missing_file.contains("missing error file"));

    let empty_model =
        parse_error_relpath("errors/invalid_auth/.json").expect_err("expected empty model");
    assert!(empty_model.contains("empty error model"));
}

#[test]
fn parse_error_relpath_rejects_non_json_suffix() {
    let error = parse_error_relpath("errors/invalid_auth/model.txt")
        .expect_err("expected non-json suffix rejection");
    assert!(error.contains("does not end with .json"));
}

#[test]
fn has_top_level_error_object_requires_error_type_and_object() {
    assert!(has_top_level_error_object(&json!({
        "type": "error",
        "error": { "message": "bad request" }
    })));
    assert!(!has_top_level_error_object(&json!({
        "error": { "message": "bad request" }
    })));
    assert!(!has_top_level_error_object(&json!({
        "type": "message",
        "error": { "message": "bad request" }
    })));
    assert!(!has_top_level_error_object(&json!({
        "type": "error",
        "error": "bad request"
    })));
}
