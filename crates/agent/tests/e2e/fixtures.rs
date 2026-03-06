use std::path::{Path, PathBuf};

use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub enum FixtureProvider {
    OpenAi,
    Anthropic,
    OpenRouter,
}

#[derive(Debug, Clone, Copy)]
pub enum FixtureScenario {
    BasicChat,
    ToolCall,
    InvalidAuth,
    InvalidRequestSchema,
    InvalidToolPayload,
    InvalidModel,
}

pub fn load_fixture_text(
    provider: FixtureProvider,
    scenario: FixtureScenario,
    model_file: &str,
) -> String {
    let fixture_path = fixture_root(provider)
        .join(scenario_segment(scenario))
        .join(model_file);

    std::fs::read_to_string(&fixture_path).unwrap_or_else(|error| {
        panic!("failed to read fixture {}: {error}", fixture_path.display())
    })
}

pub fn load_fixture_json(
    provider: FixtureProvider,
    scenario: FixtureScenario,
    model_file: &str,
) -> Value {
    let fixture_text = load_fixture_text(provider, scenario, model_file);
    let parsed: Value = serde_json::from_str(&fixture_text).unwrap_or_else(|error| {
        panic!("failed to parse fixture JSON for {provider:?}/{scenario:?}/{model_file}: {error}")
    });

    parsed
        .get("response")
        .and_then(|response| response.get("body"))
        .cloned()
        .unwrap_or(parsed)
}

fn fixture_root(provider: FixtureProvider) -> PathBuf {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../agent-providers/data");

    match provider {
        FixtureProvider::OpenAi => data_root.join("openai/responses/2026-02-27T03:25:13.281Z"),
        FixtureProvider::Anthropic => {
            data_root.join("anthropic/responses/2026-02-27T02:12:18.639Z")
        }
        FixtureProvider::OpenRouter => {
            data_root.join("openrouter/responses/2026-02-27T02:34:30.762Z")
        }
    }
}

fn scenario_segment(scenario: FixtureScenario) -> &'static str {
    match scenario {
        FixtureScenario::BasicChat => "basic_chat",
        FixtureScenario::ToolCall => "tool_call",
        FixtureScenario::InvalidAuth => "errors/invalid_auth",
        FixtureScenario::InvalidRequestSchema => "errors/invalid_request_schema",
        FixtureScenario::InvalidToolPayload => "errors/invalid_tool_payload",
        FixtureScenario::InvalidModel => "errors/invalid_model",
    }
}
