use std::panic;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::platform::test_fixtures::{
    choose_valid_success_fixture, list_error_fixture_models, list_fixture_models,
    load_error_fixture_body, load_success_fixture, resolve_fixture_responses_root_from,
    validate_error_fixture_shape, validate_error_fixture_wrapper_shape,
};

fn unique_temp_dir(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let path = std::env::temp_dir()
        .join("agent-providers-test-fixtures")
        .join(format!("{label}-{now}"));
    std::fs::create_dir_all(&path).expect("failed to create temp directory");
    path
}

fn create_dir(path: &Path) {
    std::fs::create_dir_all(path).expect("failed to create directory tree");
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).expect("failed to canonicalize path")
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    "unknown panic payload".to_string()
}

fn assert_panics_with_message<F>(f: F, needle: &str)
where
    F: FnOnce() + panic::UnwindSafe,
{
    let panic = panic::catch_unwind(f).expect_err("expected panic");
    let message = panic_text(panic);
    assert!(
        message.contains(needle),
        "panic text did not contain '{needle}', got: {message}"
    );
}

#[test]
fn resolves_workspace_root_relative_layout() {
    let root = unique_temp_dir("workspace-root");
    let expected = root
        .join("crates")
        .join("agent-providers")
        .join("data")
        .join("openai")
        .join("responses");
    create_dir(&expected);

    let cwd = root.join("deep").join("nested").join("cwd");
    create_dir(&cwd);

    let resolved = resolve_fixture_responses_root_from("openai", &cwd, None, false);
    assert_eq!(resolved, canonical(&expected));
}

#[test]
fn resolves_crate_root_relative_layout() {
    let root = unique_temp_dir("crate-root");
    let crate_root = root.join("agent-providers");
    let expected = crate_root.join("data").join("anthropic").join("responses");
    create_dir(&expected);

    let cwd = crate_root.join("src").join("platform");
    create_dir(&cwd);

    let resolved = resolve_fixture_responses_root_from("anthropic", &cwd, None, false);
    assert_eq!(resolved, canonical(&expected));
}

#[test]
fn override_takes_precedence_when_valid() {
    let root = unique_temp_dir("override-precedence");
    let cwd_root = root.join("workspace");
    let cwd_candidate = cwd_root
        .join("crates")
        .join("agent-providers")
        .join("data")
        .join("openrouter")
        .join("responses");
    create_dir(&cwd_candidate);
    let cwd = cwd_root.join("nested");
    create_dir(&cwd);

    let override_root = root.join("override");
    let override_candidate = override_root.join("openrouter").join("responses");
    create_dir(&override_candidate);

    let resolved = resolve_fixture_responses_root_from(
        "openrouter",
        &cwd,
        Some(override_root.as_path()),
        false,
    );
    assert_eq!(resolved, canonical(&override_candidate));
}

#[test]
fn falls_back_when_override_is_invalid() {
    let root = unique_temp_dir("override-fallback");
    let expected = root
        .join("crates")
        .join("agent-providers")
        .join("data")
        .join("openai")
        .join("responses");
    create_dir(&expected);
    let cwd = root.join("nested");
    create_dir(&cwd);

    let invalid_override = root.join("missing-override-root");

    let resolved = resolve_fixture_responses_root_from(
        "openai",
        &cwd,
        Some(invalid_override.as_path()),
        false,
    );
    assert_eq!(resolved, canonical(&expected));
}

#[test]
fn missing_fixtures_error_lists_attempted_paths() {
    let root = unique_temp_dir("missing-fixtures");
    let cwd = root.join("cwd");
    create_dir(&cwd);
    let override_root = root.join("override-root");

    let panic = panic::catch_unwind(|| {
        resolve_fixture_responses_root_from(
            "definitely-missing-provider",
            &cwd,
            Some(override_root.as_path()),
            false,
        )
    })
    .expect_err("expected resolver to panic");

    let message = panic_text(panic);
    assert!(message.contains("failed to resolve fixture responses root"));
    assert!(message.contains("AGENT_PROVIDERS_FIXTURE_ROOT"));
    assert!(message.contains("attempted paths:"));
    assert!(message.contains("override-root"));
}

#[test]
fn resolve_rejects_invalid_provider_segment() {
    let root = unique_temp_dir("invalid-provider");
    let cwd = root.join("cwd");
    create_dir(&cwd);

    assert_panics_with_message(
        || {
            resolve_fixture_responses_root_from("../openai", &cwd, None, false);
        },
        "invalid fixture provider segment",
    );
}

#[test]
fn fixture_accessors_reject_invalid_scenario_or_model_segments() {
    assert_panics_with_message(
        || {
            list_fixture_models("openai", "../basic_chat");
        },
        "invalid fixture scenario segment",
    );

    assert_panics_with_message(
        || {
            load_success_fixture("openai", "basic_chat", "../gpt-5-mini");
        },
        "invalid fixture model segment",
    );

    assert_panics_with_message(
        || {
            load_error_fixture_body("openai", "invalid_model/..", "this-model-does-not-exist");
        },
        "invalid fixture scenario segment",
    );

    assert_panics_with_message(
        || {
            list_error_fixture_models("openai", "invalid_model/..");
        },
        "invalid fixture scenario segment",
    );
}

#[test]
fn choose_valid_success_fixture_selects_preferred_model() {
    let models = list_fixture_models("openai", "basic_chat");
    assert!(
        !models.is_empty(),
        "expected at least one success fixture model"
    );
    let preferred = models[0].clone();

    let chosen = choose_valid_success_fixture("openai", "basic_chat", &preferred, |model, _| {
        if model == preferred {
            Ok(())
        } else {
            Err("not preferred".to_string())
        }
    });

    assert_eq!(chosen.requested_model, preferred);
    assert_eq!(chosen.chosen_model, chosen.requested_model);
    assert!(!chosen.swapped);
    assert!(chosen.preferred_rejection_reason.is_none());
    assert!(chosen.body.is_object());
}

#[test]
fn choose_valid_success_fixture_swaps_to_fallback_when_preferred_rejected() {
    let models = list_fixture_models("openai", "basic_chat");
    assert!(
        models.len() >= 2,
        "expected at least two success fixture models for swap coverage"
    );
    let preferred = models[0].clone();
    let expected_fallback = models[1].clone();

    let chosen = choose_valid_success_fixture("openai", "basic_chat", &preferred, |model, _| {
        if model == preferred {
            Err("preferred intentionally rejected".to_string())
        } else {
            Ok(())
        }
    });

    assert_eq!(chosen.requested_model, preferred);
    assert_eq!(chosen.chosen_model, expected_fallback);
    assert!(chosen.swapped);
    assert!(chosen.preferred_rejection_reason.is_none());
}

#[test]
fn choose_valid_success_fixture_panics_when_all_candidates_rejected() {
    assert_panics_with_message(
        || {
            choose_valid_success_fixture("openai", "basic_chat", "gpt-5-mini", |_model, _| {
                Err("reject all".to_string())
            });
        },
        "no valid fixture candidates",
    );
}

#[test]
fn choose_valid_success_fixture_rejects_invalid_preferred_model_segment() {
    assert_panics_with_message(
        || {
            choose_valid_success_fixture("openai", "basic_chat", "../bad", |_model, _| Ok(()));
        },
        "invalid fixture preferred model segment",
    );
}

#[test]
fn list_error_fixture_models_is_sorted() {
    let models = list_error_fixture_models("openai", "invalid_model");
    assert!(
        !models.is_empty(),
        "expected at least one error fixture model"
    );

    let mut expected = models.clone();
    expected.sort();
    assert_eq!(models, expected);
}

#[test]
fn validate_error_fixture_shape_accepts_known_good_fixture() {
    let models = list_error_fixture_models("openai", "invalid_model");
    assert!(
        !models.is_empty(),
        "expected at least one error fixture model"
    );

    let result = validate_error_fixture_shape("openai", "invalid_model", &models[0]);
    assert!(result.is_ok(), "expected known fixture wrapper to validate");
}

#[test]
fn validate_error_fixture_wrapper_shape_rejects_malformed_body() {
    let malformed = json!({
        "response": {
            "body": "not-an-object"
        }
    });
    let path = Path::new("errors/invalid_model/malformed.json");
    let err = validate_error_fixture_wrapper_shape(&malformed, path)
        .expect_err("expected malformed wrapper to fail validation");
    assert!(err.contains("response.body"));
    assert!(err.contains("must be an object"));
}
