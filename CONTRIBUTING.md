# Contributing

## Toolchain and compatibility policy

- The workspace toolchain is pinned in `rust-toolchain.toml` (`stable` channel with `rustfmt` and `clippy`).
- The workspace compatibility floor is `rust-version = "1.85"` in root `Cargo.toml`.

## Required local checks before push

Run the same deterministic checks as CI:

```bash
cargo check --workspace --all-targets --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --workspace --lib --all-features -- -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic
cargo test --workspace --all-targets --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --document-private-items
```

## Lint policy

- Workspace lint configuration is centralized in root `Cargo.toml` under `[workspace.lints.*]`.
- Every crate inherits lints via `[lints] workspace = true`.
- `clippy::unwrap_used`, `clippy::expect_used`, and `clippy::panic` are currently enforced on non-test targets.
- Tests are intentionally not in strict migration scope for this milestone.

## Live tests

- Live tests are opt-in only.
- CI `live_tests` runs only when explicitly requested by workflow dispatch input or when `RUN_LIVE_TESTS=true` repository variable is set.
- Required credentials:
  - `OPENAI_API_KEY`
  - `ANTHROPIC_API_KEY`
  - `OPENROUTER_API_KEY`
- Missing credentials produce a deterministic skip (clear message, no random failures).
- Live tests command:

```bash
cargo test -p agent_toolkit --features live-tests
```

## Branch protection expectations

- Required status checks: `quality`, `clippy_strict_non_test`, `msrv_floor`.
- Optional/manual check: `live_tests` (required only when maintainers explicitly invoke that policy path).
