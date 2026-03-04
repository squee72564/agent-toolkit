use serde_json::Value;

use crate::openai_spec::{OpenAiDecodeEnvelope, OpenAiSpecError};
use crate::platform::test_fixtures::{
    choose_valid_success_fixture, list_error_fixture_models, list_error_fixture_relpaths,
    list_fixture_models, load_error_fixture_body, load_success_fixture,
    validate_error_fixture_shape,
};
use crate::translator_contract::ProtocolTranslator;
use agent_core::types::{ContentPart, FinishReason, Response, ResponseFormat};

use super::translator::{OpenRouterTranslator, OpenRouterTranslatorError};

const PROVIDER: &str = "openrouter";
const SUCCESS_SCENARIOS: [&str; 3] = ["basic_chat", "tool_call", "tool_call_reasoning"];
const SMOKE_MODELS_OPENROUTER: [&str; 3] = [
    "openai.gpt-5-mini",
    "anthropic.claude-sonnet-4.6",
    "google.gemini-2.5-pro",
];
const SMOKE_ERROR_FIXTURES: [(&str, &str); 4] = [
    ("invalid_auth", "openai.gpt-5-mini"),
    ("invalid_model", "openai.this-model-does-not-exist"),
    ("invalid_request_schema", "openai.gpt-5-mini"),
    ("invalid_tool_payload", "openai.gpt-5-mini"),
];
const WARN_FALLBACK_CHAT_COMPLETIONS: &str = "openrouter.decode.fallback_chat_completions";
const QUARANTINED_SUCCESS_FIXTURES: [(&str, &str, &str); 1] = [(
    "tool_call",
    "openai.o4-mini-high",
    "scenario is tool_call, but fixture is incomplete length-truncated with no tool_calls",
)];

#[test]
fn fixture_smoke_openrouter_basic_chat() {
    run_success_smoke_scenario("basic_chat", &SMOKE_MODELS_OPENROUTER);
}

#[test]
fn fixture_smoke_openrouter_tool_call() {
    run_success_smoke_scenario("tool_call", &SMOKE_MODELS_OPENROUTER);
}

#[test]
fn fixture_smoke_openrouter_tool_call_reasoning() {
    run_success_smoke_scenario("tool_call_reasoning", &SMOKE_MODELS_OPENROUTER);
}

#[test]
fn fixture_smoke_openrouter_errors() {
    for (scenario, preferred_model) in SMOKE_ERROR_FIXTURES {
        let chosen_model = choose_error_fixture_model_for_upstream(scenario, preferred_model)
            .unwrap_or_else(|| {
                panic!(
                    "no valid upstream error fixture available for provider={PROVIDER} scenario={scenario} preferred={preferred_model}"
                )
            });

        let body = load_error_fixture_body(PROVIDER, scenario, &chosen_model);
        let payload = OpenAiDecodeEnvelope {
            body,
            requested_response_format: ResponseFormat::Text,
        };
        let error = OpenRouterTranslator::default()
            .decode_request(&payload)
            .expect_err("expected upstream decode error for error fixture");
        assert_openrouter_upstream_error(error, scenario, &chosen_model);
    }
}

#[test]
#[ignore]
fn fixture_full_openrouter_success_sweep() {
    for scenario in SUCCESS_SCENARIOS {
        let models = list_fixture_models(PROVIDER, scenario);
        assert!(
            !models.is_empty(),
            "expected at least one fixture model for scenario {scenario}"
        );

        let mut quarantined_seen = 0usize;
        for model in models {
            let body = load_success_fixture(PROVIDER, scenario, &model);
            if let Err(reason) = validate_success_fixture_body(&body, scenario, &model) {
                if let Some(quarantine_reason) = quarantine_success_reason(scenario, &model) {
                    quarantined_seen += 1;
                    eprintln!(
                        "quarantined success fixture skipped: provider={PROVIDER} scenario={scenario} model={model} reason={quarantine_reason}; validation_error={reason}"
                    );
                    continue;
                }
                panic!("invalid success fixture {scenario}/{model}: {reason}");
            }
        }

        let expected_quarantined = QUARANTINED_SUCCESS_FIXTURES
            .iter()
            .filter(|(s, _, _)| *s == scenario)
            .count();
        assert_eq!(
            quarantined_seen, expected_quarantined,
            "quarantined OpenRouter success fixture count changed for scenario={scenario}"
        );
    }
}

#[test]
#[ignore]
fn fixture_full_openrouter_errors_sweep() {
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

        let payload = OpenAiDecodeEnvelope {
            body,
            requested_response_format: ResponseFormat::Text,
        };
        let error = OpenRouterTranslator::default()
            .decode_request(&payload)
            .expect_err("expected upstream decode error for error fixture");
        assert_openrouter_upstream_error(error, scenario, model);
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
        if selected.swapped {
            if let Some(reason) = &selected.preferred_rejection_reason {
                eprintln!(
                    "fixture swap reason: provider={PROVIDER} scenario={scenario} requested={} chosen={} reason={reason}",
                    selected.requested_model, selected.chosen_model
                );
            }
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
    let payload = OpenAiDecodeEnvelope {
        body: body.clone(),
        requested_response_format: ResponseFormat::Text,
    };
    let response = OpenRouterTranslator::default()
        .decode_request(&payload)
        .map_err(|err| format!("decode failed: {err}"))?;
    assert_success_invariants(&response, scenario)?;
    assert_fallback_warning_for_non_openai_shape(body, &response)
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

fn assert_openrouter_upstream_error(error: OpenRouterTranslatorError, scenario: &str, model: &str) {
    match error {
        OpenRouterTranslatorError::Decode(OpenAiSpecError::Upstream { message }) => {
            assert!(
                !message.trim().is_empty(),
                "expected non-empty upstream message for {scenario}/{model}"
            );
            assert!(
                message.contains("openai error:") || message.contains("openrouter error:"),
                "expected provider error context in upstream message for {scenario}/{model}: {message}"
            );
        }
        other => {
            panic!("expected decode upstream error for fixture {scenario}/{model}, got: {other}")
        }
    }
}

fn assert_fallback_warning_for_non_openai_shape(
    body: &Value,
    response: &Response,
) -> Result<(), String> {
    if is_openai_responses_shape(body) {
        return Ok(());
    }
    if response
        .warnings
        .iter()
        .any(|warning| warning.code == WARN_FALLBACK_CHAT_COMPLETIONS)
    {
        Ok(())
    } else {
        Err(format!(
            "missing expected warning '{WARN_FALLBACK_CHAT_COMPLETIONS}' for fallback-shaped response"
        ))
    }
}

fn is_openai_responses_shape(body: &Value) -> bool {
    let Some(root) = body.as_object() else {
        return false;
    };
    root.get("status").is_some() && root.get("output").is_some_and(Value::is_array)
}

fn has_top_level_error_object(body: &Value) -> bool {
    body.get("error").is_some_and(Value::is_object)
}

fn quarantine_success_reason(scenario: &str, model: &str) -> Option<&'static str> {
    QUARANTINED_SUCCESS_FIXTURES
        .iter()
        .find(|(s, m, _)| *s == scenario && *m == model)
        .map(|(_, _, reason)| *reason)
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
