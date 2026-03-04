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
