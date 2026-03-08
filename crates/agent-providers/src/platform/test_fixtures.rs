use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct ChosenFixture {
    pub requested_model: String,
    pub chosen_model: String,
    pub body: Value,
    pub swapped: bool,
    pub preferred_rejection_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum FixtureSet {
    Decoded,
    Streaming,
}

fn fixture_responses_root(provider: &str) -> PathBuf {
    resolve_fixture_responses_root(provider)
}

fn assert_valid_fixture_segment(label: &str, value: &str) {
    assert!(
        !value.is_empty(),
        "invalid fixture {label} segment: empty value"
    );
    assert!(
        value != "." && value != "..",
        "invalid fixture {label} segment '{value}': relative path markers are not allowed"
    );
    assert!(
        !value.contains('/') && !value.contains('\\'),
        "invalid fixture {label} segment '{value}': path separators are not allowed"
    );
}

fn assert_valid_provider(provider: &str) {
    assert_valid_fixture_segment("provider", provider);
}

pub(crate) fn resolve_fixture_responses_root(provider: &str) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|err| {
        panic!("failed to determine current working directory for fixture discovery: {err}")
    });
    let env_override = std::env::var_os("AGENT_PROVIDERS_FIXTURE_ROOT").map(PathBuf::from);
    resolve_fixture_responses_root_from(provider, &cwd, env_override.as_deref(), true)
}

pub(crate) fn resolve_fixture_responses_root_from(
    provider: &str,
    cwd: &Path,
    env_override: Option<&Path>,
    include_manifest_fallback: bool,
) -> PathBuf {
    assert_valid_provider(provider);

    let mut attempted = Vec::new();

    if let Some(root) = env_override {
        let primary = root.join(provider).join("responses");
        attempted.push(primary.clone());
        if primary.is_dir() {
            return canonicalize_if_possible(primary);
        }

        let fallback = root.join("data").join(provider).join("responses");
        attempted.push(fallback.clone());
        if fallback.is_dir() {
            return canonicalize_if_possible(fallback);
        }
    }

    for base in ancestry_from(cwd) {
        let data_relative = base.join("data").join(provider).join("responses");
        attempted.push(data_relative.clone());
        if data_relative.is_dir() {
            return canonicalize_if_possible(data_relative);
        }

        let workspace_relative = base
            .join("crates")
            .join("agent-providers")
            .join("data")
            .join(provider)
            .join("responses");
        attempted.push(workspace_relative.clone());
        if workspace_relative.is_dir() {
            return canonicalize_if_possible(workspace_relative);
        }
    }

    if include_manifest_fallback && let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        for base in ancestry_from(Path::new(manifest_dir)) {
            let data_relative = base.join("data").join(provider).join("responses");
            attempted.push(data_relative.clone());
            if data_relative.is_dir() {
                return canonicalize_if_possible(data_relative);
            }

            let workspace_relative = base
                .join("crates")
                .join("agent-providers")
                .join("data")
                .join(provider)
                .join("responses");
            attempted.push(workspace_relative.clone());
            if workspace_relative.is_dir() {
                return canonicalize_if_possible(workspace_relative);
            }
        }
    }

    panic!(
        "failed to resolve fixture responses root for provider='{provider}'. current_dir='{}'. AGENT_PROVIDERS_FIXTURE_ROOT={}. attempted paths:\n{}",
        cwd.display(),
        env_override.map_or_else(|| "<unset>".to_string(), |path| path.display().to_string()),
        format_attempted_paths(&attempted)
    );
}

fn ancestry_from(start: &Path) -> Vec<PathBuf> {
    let mut ancestry = Vec::new();
    let mut current = Some(start);
    while let Some(path) = current {
        ancestry.push(path.to_path_buf());
        current = path.parent();
    }
    ancestry
}

fn canonicalize_if_possible(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn format_attempted_paths(attempted: &[PathBuf]) -> String {
    if attempted.is_empty() {
        return "  (none)".to_string();
    }

    attempted
        .iter()
        .map(|path| format!("  - {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_json(path: &Path) -> Value {
    assert!(path.is_file(), "missing fixture file: {}", path.display());
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read fixture file {}: {err}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse fixture JSON at {}: {err}", path.display()))
}

fn fixture_set_root(provider: &str, fixture_set: FixtureSet) -> PathBuf {
    assert_valid_provider(provider);

    let responses_root = fixture_responses_root(provider);
    let dir_name = match fixture_set {
        FixtureSet::Decoded => "decoded",
        FixtureSet::Streaming => "streaming",
    };
    let fixture_root = responses_root.join(dir_name);
    assert!(
        fixture_root.is_dir(),
        "missing fixture set directory for provider='{provider}' set='{dir_name}': {}",
        fixture_root.display()
    );
    canonicalize_if_possible(fixture_root)
}

fn decoded_fixture_root(provider: &str) -> PathBuf {
    fixture_set_root(provider, FixtureSet::Decoded)
}

#[allow(dead_code)]
pub(crate) fn streaming_fixture_root(provider: &str) -> PathBuf {
    fixture_set_root(provider, FixtureSet::Streaming)
}

fn decoded_error_fixture_path(provider: &str, scenario: &str, model: &str) -> PathBuf {
    assert_valid_provider(provider);
    assert_valid_fixture_segment("scenario", scenario);
    assert_valid_fixture_segment("model", model);

    decoded_fixture_root(provider)
        .join("errors")
        .join(scenario)
        .join(format!("{model}.json"))
}

pub(crate) fn load_decoded_success_fixture(provider: &str, scenario: &str, model: &str) -> Value {
    assert_valid_provider(provider);
    assert_valid_fixture_segment("scenario", scenario);
    assert_valid_fixture_segment("model", model);

    let path = decoded_fixture_root(provider)
        .join(scenario)
        .join(format!("{model}.json"));
    read_json(&path)
}

pub(crate) fn load_decoded_error_fixture_body(
    provider: &str,
    scenario: &str,
    model: &str,
) -> Value {
    assert_valid_provider(provider);
    assert_valid_fixture_segment("scenario", scenario);
    assert_valid_fixture_segment("model", model);

    let path = decoded_error_fixture_path(provider, scenario, model);
    let fixture = read_json(&path);
    fixture
        .get("response")
        .and_then(|response| response.get("body"))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "missing response.body in error fixture wrapper: {}",
                path.display()
            )
        })
}

pub(crate) fn list_decoded_fixture_models(provider: &str, scenario: &str) -> Vec<String> {
    assert_valid_provider(provider);
    assert_valid_fixture_segment("scenario", scenario);

    let scenario_dir = decoded_fixture_root(provider).join(scenario);
    assert!(
        scenario_dir.is_dir(),
        "missing fixture scenario directory: {}",
        scenario_dir.display()
    );

    let mut models = fs::read_dir(&scenario_dir)
        .unwrap_or_else(|err| {
            panic!(
                "failed to list fixture scenario directory {}: {err}",
                scenario_dir.display()
            )
        })
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                return None;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();

    models.sort();
    models
}

pub(crate) fn load_decoded_success_fixture_candidates(
    provider: &str,
    scenario: &str,
) -> Vec<(String, Value)> {
    assert_valid_provider(provider);
    assert_valid_fixture_segment("scenario", scenario);

    let models = list_decoded_fixture_models(provider, scenario);
    models
        .into_iter()
        .map(|model| {
            let body = load_decoded_success_fixture(provider, scenario, &model);
            (model, body)
        })
        .collect()
}

pub(crate) fn choose_valid_success_fixture<F>(
    provider: &str,
    scenario: &str,
    preferred_model: &str,
    mut validator: F,
) -> ChosenFixture
where
    F: FnMut(&str, &Value) -> Result<(), String>,
{
    assert_valid_provider(provider);
    assert_valid_fixture_segment("scenario", scenario);
    assert_valid_fixture_segment("preferred model", preferred_model);

    let candidates = load_decoded_success_fixture_candidates(provider, scenario);
    assert!(
        !candidates.is_empty(),
        "no success fixtures found for provider={provider} scenario={scenario}"
    );

    let mut preferred: Option<(String, Value)> = None;
    let mut others = Vec::new();
    for (model, body) in candidates {
        if model == preferred_model {
            preferred = Some((model, body));
        } else {
            others.push((model, body));
        }
    }

    let mut ordered = Vec::new();
    let mut preferred_missing_reason = None;
    if let Some(candidate) = preferred {
        ordered.push(candidate);
    } else {
        preferred_missing_reason = Some(format!(
            "preferred model '{preferred_model}' not present in scenario fixtures"
        ));
    }
    ordered.extend(others);

    let mut rejected = Vec::new();
    for (model, body) in ordered {
        match validator(&model, &body) {
            Ok(()) => {
                let swapped = model != preferred_model;
                if swapped {
                    eprintln!(
                        "fixture swap: provider={provider} scenario={scenario} requested={preferred_model} chosen={model}"
                    );
                }
                return ChosenFixture {
                    requested_model: preferred_model.to_string(),
                    chosen_model: model,
                    body,
                    swapped,
                    preferred_rejection_reason: preferred_missing_reason,
                };
            }
            Err(reason) => rejected.push(format!("{model}: {reason}")),
        }
    }

    panic!(
        "no valid fixture candidates for provider={provider} scenario={scenario} requested={preferred_model}; rejected: [{}]",
        rejected.join("; ")
    );
}

pub(crate) fn list_decoded_error_fixture_models(provider: &str, scenario: &str) -> Vec<String> {
    assert_valid_provider(provider);
    assert_valid_fixture_segment("scenario", scenario);

    let scenario_dir = decoded_fixture_root(provider).join("errors").join(scenario);
    assert!(
        scenario_dir.is_dir(),
        "missing fixture error scenario directory: {}",
        scenario_dir.display()
    );

    let mut models = fs::read_dir(&scenario_dir)
        .unwrap_or_else(|err| {
            panic!(
                "failed to list fixture error scenario directory {}: {err}",
                scenario_dir.display()
            )
        })
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                return None;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();

    models.sort();
    models
}

pub(crate) fn list_decoded_error_fixture_relpaths(provider: &str) -> Vec<String> {
    assert_valid_provider(provider);

    let errors_dir = decoded_fixture_root(provider).join("errors");
    assert!(
        errors_dir.is_dir(),
        "missing fixture errors directory: {}",
        errors_dir.display()
    );

    let mut relpaths = Vec::new();
    let scenarios = fs::read_dir(&errors_dir)
        .unwrap_or_else(|err| {
            panic!(
                "failed to list fixture errors directory {}: {err}",
                errors_dir.display()
            )
        })
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            Some(entry.file_name().to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();

    for scenario in scenarios {
        let scenario_dir = errors_dir.join(&scenario);
        let files = fs::read_dir(&scenario_dir)
            .unwrap_or_else(|err| {
                panic!(
                    "failed to list fixture error scenario directory {}: {err}",
                    scenario_dir.display()
                )
            })
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_file() {
                    return None;
                }
                let path = entry.path();
                let file_name = path.file_name()?.to_string_lossy().to_string();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    return None;
                }
                Some(file_name)
            })
            .collect::<Vec<_>>();

        for file_name in files {
            relpaths.push(format!("errors/{scenario}/{file_name}"));
        }
    }

    relpaths.sort();
    relpaths
}

pub(crate) fn validate_decoded_error_fixture_shape(
    provider: &str,
    scenario: &str,
    model: &str,
) -> Result<(), String> {
    assert_valid_provider(provider);
    assert_valid_fixture_segment("scenario", scenario);
    assert_valid_fixture_segment("model", model);

    let path = decoded_error_fixture_path(provider, scenario, model);
    let fixture = read_json(&path);
    validate_error_fixture_wrapper_shape(&fixture, &path)
}

pub(crate) fn validate_error_fixture_wrapper_shape(
    fixture: &Value,
    path: &Path,
) -> Result<(), String> {
    let root = fixture
        .as_object()
        .ok_or_else(|| format!("fixture wrapper is not an object: {}", path.display()))?;
    let response = root
        .get("response")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!(
                "fixture wrapper missing object field 'response': {}",
                path.display()
            )
        })?;
    let body = response.get("body").ok_or_else(|| {
        format!(
            "fixture wrapper missing field 'response.body': {}",
            path.display()
        )
    })?;
    if !body.is_object() {
        return Err(format!(
            "fixture wrapper field 'response.body' must be an object: {}",
            path.display()
        ));
    }
    Ok(())
}
