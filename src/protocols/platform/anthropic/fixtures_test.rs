use serde_json::Value;

use crate::core::types::{ContentPart, FinishReason, Response, ResponseFormat};
use crate::protocols::anthropic_spec::{AnthropicDecodeEnvelope, AnthropicSpecError};
use crate::protocols::platform::test_fixtures::{
    choose_valid_success_fixture, list_error_fixture_models, list_error_fixture_relpaths,
    list_fixture_models, load_error_fixture_body, load_success_fixture,
    validate_error_fixture_shape,
};
use crate::protocols::translator_contract::ProtocolTranslator;

use super::translator::{AnthropicTranslator, AnthropicTranslatorError};

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
fn fixture_smoke_anthropic_basic_chat() {
    run_success_smoke_scenario("basic_chat", &SMOKE_MODELS_ANTHROPIC);
}

#[test]
fn fixture_smoke_anthropic_tool_call() {
    run_success_smoke_scenario("tool_call", &SMOKE_MODELS_ANTHROPIC);
}

#[test]
fn fixture_smoke_anthropic_tool_call_reasoning() {
    run_success_smoke_scenario("tool_call_reasoning", &SMOKE_MODELS_ANTHROPIC);
}

#[test]
fn fixture_smoke_anthropic_errors() {
    for (scenario, preferred_model) in SMOKE_ERROR_FIXTURES {
        let chosen_model = choose_error_fixture_model_for_upstream(scenario, preferred_model)
            .unwrap_or_else(|| {
                panic!(
                    "no valid upstream error fixture available for provider={PROVIDER} scenario={scenario} preferred={preferred_model}"
                )
            });

        let body = load_error_fixture_body(PROVIDER, scenario, &chosen_model);
        let payload = AnthropicDecodeEnvelope {
            body,
            requested_response_format: ResponseFormat::Text,
        };
        let error = AnthropicTranslator
            .decode_request(&payload)
            .expect_err("expected upstream decode error for error fixture");
        assert_anthropic_upstream_error(error, scenario, &chosen_model);
    }
}

#[test]
#[ignore]
fn fixture_full_anthropic_success_sweep() {
    for scenario in SUCCESS_SCENARIOS {
        let models = list_fixture_models(PROVIDER, scenario);
        assert!(
            !models.is_empty(),
            "expected at least one fixture model for scenario {scenario}"
        );
        for model in models {
            let body = load_success_fixture(PROVIDER, scenario, &model);
            validate_success_fixture_body(&body, scenario, &model).unwrap_or_else(|reason| {
                panic!("invalid success fixture {scenario}/{model}: {reason}")
            });
        }
    }
}

#[test]
#[ignore]
fn fixture_full_anthropic_errors_sweep() {
    let relpaths = list_error_fixture_relpaths(PROVIDER);
    assert!(
        !relpaths.is_empty(),
        "expected at least one error fixture relpath for provider {PROVIDER}"
    );
    for relpath in relpaths {
        let (scenario, model) = parse_error_relpath(&relpath);
        validate_error_fixture_shape(PROVIDER, scenario, model).unwrap_or_else(|reason| {
            panic!("invalid error fixture wrapper {scenario}/{model}: {reason}")
        });

        let body = load_error_fixture_body(PROVIDER, scenario, model);
        assert!(
            has_top_level_error_object(&body),
            "error fixture missing top-level error object: {scenario}/{model}"
        );

        let payload = AnthropicDecodeEnvelope {
            body,
            requested_response_format: ResponseFormat::Text,
        };
        let error = AnthropicTranslator
            .decode_request(&payload)
            .expect_err("expected upstream decode error for error fixture");
        assert_anthropic_upstream_error(error, scenario, model);
    }
}

fn run_success_smoke_scenario(scenario: &str, preferred_models: &[&str]) {
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
        validate_success_fixture_body(&selected.body, scenario, &selected.chosen_model)
            .unwrap_or_else(|reason| {
                panic!(
                    "selected fixture failed validation {scenario}/{}: {reason}",
                    selected.chosen_model
                )
            });
    }
}

fn choose_error_fixture_model_for_upstream(
    scenario: &str,
    preferred_model: &str,
) -> Option<String> {
    let mut models = list_error_fixture_models(PROVIDER, scenario);
    if let Some(pos) = models.iter().position(|model| model == preferred_model) {
        let preferred = models.remove(pos);
        models.insert(0, preferred);
    }

    for model in models {
        validate_error_fixture_shape(PROVIDER, scenario, &model).unwrap_or_else(|reason| {
            panic!("invalid error fixture wrapper {scenario}/{model}: {reason}")
        });
        let body = load_error_fixture_body(PROVIDER, scenario, &model);
        if has_top_level_error_object(&body) {
            if model != preferred_model {
                eprintln!(
                    "error fixture swap: provider={PROVIDER} scenario={scenario} requested={preferred_model} chosen={model}"
                );
            }
            return Some(model);
        }
    }
    None
}

fn validate_success_fixture_body(body: &Value, scenario: &str, _model: &str) -> Result<(), String> {
    let payload = AnthropicDecodeEnvelope {
        body: body.clone(),
        requested_response_format: ResponseFormat::Text,
    };
    let response = AnthropicTranslator
        .decode_request(&payload)
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

fn assert_anthropic_upstream_error(error: AnthropicTranslatorError, scenario: &str, model: &str) {
    match error {
        AnthropicTranslatorError::Decode(AnthropicSpecError::Upstream { message }) => {
            assert!(
                !message.trim().is_empty(),
                "expected non-empty upstream message for {scenario}/{model}"
            );
            assert!(
                message.contains("anthropic error:"),
                "expected provider context in upstream message for {scenario}/{model}: {message}"
            );
        }
        other => {
            panic!("expected decode upstream error for fixture {scenario}/{model}, got: {other}")
        }
    }
}

fn has_top_level_error_object(body: &Value) -> bool {
    body.get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == "error")
        && body.get("error").is_some_and(Value::is_object)
}

fn parse_error_relpath(relpath: &str) -> (&str, &str) {
    let mut parts = relpath.split('/');
    let prefix = parts.next();
    let scenario = parts.next();
    let file = parts.next();
    let extra = parts.next();

    assert_eq!(
        prefix,
        Some("errors"),
        "unexpected error relpath prefix: {relpath}"
    );
    assert!(extra.is_none(), "unexpected error relpath shape: {relpath}");

    let scenario =
        scenario.unwrap_or_else(|| panic!("missing error scenario in relpath: {relpath}"));
    let file = file.unwrap_or_else(|| panic!("missing error file in relpath: {relpath}"));
    let model = file
        .strip_suffix(".json")
        .unwrap_or_else(|| panic!("error relpath does not end with .json: {relpath}"));

    (scenario, model)
}
