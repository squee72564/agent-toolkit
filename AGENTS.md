# AGENTS.md


## Rust Commands
Use these before submitting changes:

```bash
cargo check --all-targets
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

(If needed to apply formatting:)

```bash
cargo fmt --all
```

## Test Organization
Keep tests out of implementation files to reduce file bloat and keep production code focused.

- Do not add `#[cfg(test)] mod tests` blocks inside `src/lib.rs`, `src/mod.rs`, or other implementation files.
- Put tests in dedicated test files, not inline modules.
- Preferred locations:
  - crate-level/integration tests in `tests/*.rs`
  - module-level tests in sibling files such as `src/**/test.rs` or `src/**/*_test.rs`
