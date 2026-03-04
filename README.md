# agent_toolkit

Minimal Rust workspace for provider-agnostic agent infrastructure.

## Workspace Layout

```text
crates
├── agent
│   ├── Cargo.toml
│   └── src
│       ├── lib.rs
│       └── test.rs
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
│       ├── adapter
│       │   └── test.rs
│       ├── adapter.rs
│       ├── anthropic_spec
│       │   ├── decode.rs
│       │   ├── encode.rs
│       │   ├── mod.rs
│       │   ├── schema_rules.rs
│       │   └── test.rs
│       ├── error.rs
│       ├── lib.rs
│       ├── openai_spec
│       │   ├── decode.rs
│       │   ├── encode.rs
│       │   ├── mod.rs
│       │   ├── schema_rules.rs
│       │   └── test.rs
│       ├── platform
│       │   ├── anthropic
│       │   ├── openai
│       │   ├── openrouter
│       │   ├── mod.rs
│       │   └── test_fixtures.rs
│       └── translator_contract.rs
├── agent-runtime
│   ├── Cargo.toml
│   └── src
│       ├── lib.rs
│       └── test.rs
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

- `agent` (`agent_toolkit`): facade crate with public re-exports for core, runtime, providers, transport, and tools.
- `agent-core`: provider-agnostic domain types and traits shared across crates, including canonical `ProviderId`.
- `agent-providers`: provider-specific encode/decode/spec logic, static `ProviderAdapter` lookup boundary, and fixture datasets for validation tests.
- `agent-runtime`: high-level clients (`openai()`, `anthropic()`, `openrouter()`), toolkit routing/fallback orchestration, and unified adapter-driven execution flow.
- `agent-transport`: HTTP transport implementation with retry support, auth/header handling, and JSON request/response helpers.
- `agent-tools`: lightweight tool trait and registry primitives for tool integration.
