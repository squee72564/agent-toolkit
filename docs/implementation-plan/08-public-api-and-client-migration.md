# Phase 8: Public API and Client Migration

## Goal

Preserve the ergonomic public entrypoints while normalizing all execution into
the new internal triad:

- `TaskRequest`
- `Route`
- `ExecutionOptions`

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- `MessageCreateInput` normalization
- the locked `MessageCreateInput` boundary that keeps provider-specific fields
  out of the generic task builder and keeps provider payload metadata on
  `TaskRequest` / `MessageCreateInput`
- direct-client ergonomics
- direct-client native-option convenience on concrete client APIs
- routed toolkit ergonomics
- inferred `ExecutionOptions` defaults for `.messages()` and `.streaming()`,
  including `observer: None` and `transport: TransportOptions::default()`
- explicit routed execution overloads that accept execution options
- the locked routed API boundary where `Route` remains routing-only and
  execution overrides stay on `ExecutionOptions`
- direct client model selection normalized into a generated single-attempt route
- direct client instance selection normalized into a generated single-attempt
  route targeting the configured `ProviderInstanceId`
- low-level public API replacement of monolithic request-like entrypoints with
  `TaskRequest`, `Route`, and `ExecutionOptions`
- temporary public-API migration shims that normalize legacy `Request`,
  `SendOptions`, and provider-kind-targeted route inputs into the new model

## Current Repo Anchors

- [crates/agent-runtime/src/message_create_input.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/message_create_input.rs)
- [crates/agent-runtime/src/direct_messages_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/direct_messages_api.rs)
- [crates/agent-runtime/src/direct_streaming_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/direct_streaming_api.rs)
- [crates/agent-runtime/src/routed_messages_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/routed_messages_api.rs)
- [crates/agent-runtime/src/routed_streaming_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/routed_streaming_api.rs)
- [crates/agent-runtime/src/provider_client.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_client.rs)

## Planned Additions

- Add explicit normalization paths from `MessageCreateInput` to `TaskRequest`.
- Generate single-attempt routes for direct clients.
- Keep provider-specific knobs off `MessageCreateInput`; expose native-option
  convenience only on concrete client surfaces.
- Add explicit routed overloads with caller-supplied `ExecutionOptions`.
- Add low-level explicit entrypoints that accept `TaskRequest`,
  attempt-local/direct execution inputs, and `ExecutionOptions` without
  reintroducing a monolithic `Request`.
- If compatibility shims are needed during migration, normalize legacy
  `Request`, `SendOptions`, and provider-kind-targeted route inputs into the
  new internal model and mark them for phase-10 removal.
- Retire `SendOptions` from the final public architecture.

## File-Sized Steps

1. Update `MessageCreateInput` so semantic fields normalize only into
   `TaskRequest`.
2. Move model selection out of `TaskRequest` and into generated route targets
   for direct clients.
3. Move `stream` handling out of request-like types and into inferred
   `ExecutionOptions.response_mode`.
4. Update direct messages and direct streaming APIs so they internally construct
   a single-attempt `Route`.
5. Add or update direct-client native-option convenience entrypoints so they
   normalize into attempt-local native options rather than mutating
   `MessageCreateInput`.
6. Add or update low-level direct APIs so they accept `TaskRequest`,
   `ExecutionOptions`, and explicit one-off model/attempt overrides without
   reintroducing a monolithic request type.
7. Update routed APIs so the ergonomic overloads infer default
   `ExecutionOptions` with `observer: None` and
   `TransportOptions::default()`, and the explicit overloads accept them
   directly.
8. Add or update low-level routed APIs so they accept `TaskRequest`, `Route`,
   and `ExecutionOptions` as the public explicit execution surface.
9. If temporary compatibility is required, add narrowly scoped shims that
   normalize legacy `Request`, `SendOptions`, and provider-kind-targeted route
   inputs into `TaskRequest`, `Route`, and `ExecutionOptions`.
10. Remove reliance on `SendOptions` as the main public execution override
    path.
11. Update `AgentToolkit` and client builders so configured instance identity is
   preserved in the generated route.

## Locked Rules To Encode

- direct clients remain ergonomic
- routed clients remain ergonomic
- `Route` remains routing-only at the public API boundary
- `MessageCreateInput` remains provider-agnostic and owns provider payload
  metadata, not route/execution concerns
- native option convenience lives on concrete client APIs, not on
  `MessageCreateInput`
- route-wide observer and transport overrides live on `ExecutionOptions`
- model selection is on the attempt target path, not the task
- no monolithic replacement `Request` type becomes the canonical public model
- any migration compatibility shims are temporary normalization layers, not
  target architecture

## Repo-Structure Guidance

- keep user-facing builder ergonomics thin; avoid duplicating planner logic in
  API wrappers
- normalize into the same internal execution path for direct and routed flows
- keep migration-only overloads clearly marked if they exist temporarily

## Exit Criteria

- public APIs normalize into the new internal model
- direct and routed entrypoints remain ergonomic
- low-level explicit APIs use `TaskRequest`, `Route`, and `ExecutionOptions`
  instead of a replacement monolithic `Request`
- any temporary legacy-input shims normalize into the new model and are clearly
  marked for phase-10 removal
- `SendOptions` is no longer the conceptual center of per-call execution
