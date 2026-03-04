use std::panic;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::platform::test_fixtures::resolve_fixture_responses_root_from;

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
