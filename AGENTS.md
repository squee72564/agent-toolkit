# AGENTS.md


## Rust Commands
Use these before submitting changes:

```bash
cargo check --workspace --all-targets --locked
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --workspace --lib --all-features -- -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic
cargo test --workspace --all-targets --all-features -- --quiet
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --document-private-items
```

## Test Organization
Keep tests out of implementation files to reduce file bloat and keep production code focused.

- Do not add `#[cfg(test)] mod tests` blocks inside `src/lib.rs`, `src/mod.rs`, or other implementation files.
- Put tests in dedicated test files, not inline modules.
- Preferred locations:
  - crate-level/integration tests in `tests/*.rs`
  - module-level tests in sibling files such as `src/**/test.rs` or `src/**/*_test.rs`
