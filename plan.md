# Rust Hardening Plan and File-by-File Checklist

## Summary
Harden every Rust file in a controlled, repeatable cycle: discover risk, apply focused fixes, add tests, and gate with CI checks. Track progress per file so the repo can be worked one-by-one without losing coverage.

## Assumptions and Defaults
1. No API behavior change unless documented and intentional.
2. Public API surface remains stable; prefer additive changes over breaking signature changes.
3. Existing tests are baseline and must continue passing.
4. Tests live in dedicated test files, not inline `#[cfg(test)]` blocks in implementation files.
5. Correctness and safety take priority over micro-optimizations unless a hotspot is measured.
6. Never concede on test correctness or robustness in order to make tests pass

## Global Baseline and Risk Map
1. Run repository-wide checks to build an inventory: `cargo check`, `cargo clippy`, warnings, and security checks.
2. Classify issues by severity: correctness, safety/security, concurrency, performance, lint/style masking defects.
3. Tag files by risk profile: high, medium, low.
4. Prioritize core and boundary files before leaf utilities.

## Per-File Hardening Checklist (apply to each `.rs` file)
1. Imports and dependencies: remove dead imports and tighten error typing.
2. Error handling: replace `unwrap` and `expect` unless impossible and documented.
3. Panic policy: avoid `panic!` in library paths; return typed errors.
4. Input validation: validate parse, conversion, range, and cast boundaries.
5. Invariants: enforce runtime invariants explicitly; use `debug_assert!` only for debug-only assumptions.
6. Ownership and allocations: remove unnecessary clones and avoid avoidable allocations.
7. Concurrency: keep lock scope minimal and do not hold locks across `.await`.
8. Control flow: make enum/state handling explicit and exhaustive.
9. Unsafe blocks: keep minimal, document invariants, and validate preconditions.
10. API clarity: keep return types informative and document preconditions/postconditions.

## Test Expansion Protocol
1. Add or extend tests in dedicated test files for each behavioral fix.
2. Add regression tests reproducing the original bug/edge case.
3. Add negative tests for new validation paths.
4. Add property or fuzz tests where parsing or transformation risk is high and project policy permits.
5. Add targeted benchmarks only when claiming performance changes.

## File-Level Execution Pattern
1. Pick one file.
2. Apply high-confidence hardening fixes.
3. Add or update tests in dedicated test files.
4. Run focused checks for impacted crate/module.
5. Record deltas, then mark the file checkbox complete.

## Continuous Hardening Gates
1. `cargo check --all-targets`
2. `cargo fmt --all --check`
3. `cargo clippy --all-targets --all-features -- -D warnings`
4. `cargo test --all-targets --all-features`

## Scope and Filters for File Tracking
1. Include all `.rs` files in the repo, including test files under `tests/`, `test.rs`, and `*_test.rs`.
2. Respect `.gitignore`.
3. Exclude non-relevant directories from tree discovery: `target/`, `.git/`, and `data/`.

## Rust File Tree (reference)
```text
.
+- crates
   +- agent
   |  +- src
   |     +- lib.rs
   |     +- test.rs
   +- agent-core
   |  +- src
   |  |  +- error
   |  |  |  +- mod.rs
   |  |  +- lib.rs
   |  |  +- traits
   |  |  |  +- mod.rs
   |  |  +- types
   |  |     +- mod.rs
   |  +- tests
   |     +- message_helpers_test.rs
   +- agent-providers
   |  +- src
   |     +- adapter
   |     |  +- test.rs
   |     +- adapter.rs
   |     +- anthropic_spec
   |     |  +- decode.rs
   |     |  +- encode.rs
   |     |  +- mod.rs
   |     |  +- schema_rules.rs
   |     |  +- test.rs
   |     +- error.rs
   |     +- lib.rs
   |     +- openai_spec
   |     |  +- decode.rs
   |     |  +- encode.rs
   |     |  +- mod.rs
   |     |  +- schema_rules.rs
   |     |  +- test.rs
   |     +- platform
   |     |  +- anthropic
   |     |  |  +- fixtures_test.rs
   |     |  |  +- mod.rs
   |     |  |  +- test.rs
   |     |  |  +- translator.rs
   |     |  +- mod.rs
   |     |  +- openai
   |     |  |  +- fixtures_test.rs
   |     |  |  +- mod.rs
   |     |  |  +- test.rs
   |     |  |  +- translator.rs
   |     |  +- openrouter
   |     |  |  +- fixtures_test.rs
   |     |  |  +- mod.rs
   |     |  |  +- test.rs
   |     |  |  +- translator.rs
   |     |  +- test_fixtures.rs
   |     |  +- test_fixtures_test.rs
   |     +- translator_contract.rs
   +- agent-runtime
   |  +- src
   |  |  +- lib.rs
   |  |  +- test.rs
   |  +- tests
   |     +- observer_test.rs
   +- agent-tools
   |  +- src
   |  |  +- builder.rs
   |  |  +- lib.rs
   |  |  +- schema.rs
   |  +- tests
   |     +- registry_test.rs
   |     +- schema_test.rs
   |     +- tool_builder_test.rs
   +- agent-transport
      +- src
         +- http
         |  +- mod.rs
         +- lib.rs
```

## Per-File Checklist
### crate: `agent`
- COMPLETED

### crate: `agent-core`
- COMPLETED

### crate: `agent-providers`
- COMPLETED

### crate: `agent-runtime`
- [x] crates/agent-runtime/src/lib.rs
- [x] crates/agent-runtime/src/test.rs
- [x] crates/agent-runtime/tests/observer_test.rs

### crate: `agent-tools`
- [x] crates/agent-tools/src/builder.rs
- [x] crates/agent-tools/src/lib.rs
- [x] crates/agent-tools/src/schema.rs
- [x] crates/agent-tools/tests/registry_test.rs
- [x] crates/agent-tools/tests/schema_test.rs
- [x] crates/agent-tools/tests/tool_builder_test.rs

### crate: `agent-transport`
- [x] crates/agent-transport/src/http/mod.rs
- [x] crates/agent-transport/src/http/tests.rs
- [x] crates/agent-transport/src/lib.rs

## Refresh Commands
```bash
# Regenerate tree reference
tree -L 6 --gitignore -P '*.rs' -I 'target|.git|data' --prune --noreport

# Regenerate checklist lines
rg --files -g '*.rs' | sort | sed 's#^#- [ ] #'
```

## Usage Notes
1. Check a file only when hardening and tests for that file are complete.
2. If a new `.rs` file is added, append it to this checklist in the same change.
