# agent_toolkit

Minimal Rust workspace for provider-agnostic agent infrastructure.

## Workspace Layout

```text
crates
├── agent
│   ├── Cargo.toml
│   └── src
│       └── lib.rs
├── agent-core
│   ├── Cargo.toml
│   └── src
│       ├── error
│       ├── lib.rs
│       ├── traits
│       └── types
├── agent-providers
│   ├── Cargo.toml
│   ├── data
│   │   ├── anthropic
│   │   ├── openai
│   │   └── openrouter
│   └── src
│       ├── anthropic_spec
│       ├── error.rs
│       ├── lib.rs
│       ├── openai_spec
│       ├── platform
│       └── translator_contract.rs
├── agent-runtime
│   ├── Cargo.toml
│   └── src
│       └── lib.rs
├── agent-tools
│   ├── Cargo.toml
│   └── src
│       └── lib.rs
└── agent-transport
    ├── Cargo.toml
    └── src
        ├── http
        └── lib.rs
```

## Crates

- `agent` (`agent_toolkit`): facade crate with public re-exports.
- `agent-core`: provider-agnostic core types, traits, and shared errors.
- `agent-providers`: provider protocol adapters/translators + fixtures.
- `agent-runtime`: client/runtime orchestration, routing, and fallback.
- `agent-transport`: HTTP transport, retries, auth/header handling.
- `agent-tools`: tool trait + registry primitives.
