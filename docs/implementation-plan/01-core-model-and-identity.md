# Phase 1: Core Model and Identity

## Goal

Introduce the core architectural split from
[docs/REFACTOR.md](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/docs/REFACTOR.md):

- `TaskRequest` describes what to do
- `Route` describes where it can run
- `ExecutionOptions` describes how it executes
- `ProviderFamilyId`, `ProviderKind`, and `ProviderInstanceId` describe
  distinct identity layers

This phase exists to remove the current overload where one request path still
mixes semantics, target selection, model selection, fallback, streaming, and
observer overrides.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- core architectural rules 1 through 4
- task layer, routing layer, execution layer ownership
- `TaskRequest`
- `MessageCreateInput` normalization into `TaskRequest`
- `ResponseMode`
- `TransportOptions`
- `ExecutionOptions`
- `ProviderFamilyId`
- `ProviderKind`
- `ProviderInstanceId`
- `RegisteredProvider`
- `ProviderDescriptor`, `ProviderConfig`, and `PlatformConfig` ownership split
- breaking changes around `Request`, `SendOptions`, `model_id`, `stream`, and
  provider identity
- the replacement rule that the low-level public model is
  `TaskRequest + Route + ExecutionOptions`, with no new monolithic `Request`

## Current Repo Anchors

- [crates/agent-runtime/src/message_create_input.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/message_create_input.rs)
- [crates/agent-runtime/src/send_options.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/send_options.rs)
- [crates/agent-runtime/src/target.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/target.rs)
- [crates/agent-runtime/src/provider_config.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_config.rs)
- [crates/agent-providers/src/adapter.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/adapter.rs)

## Planned Additions

- Add a runtime-owned module area for the new core public model.
- Add a runtime-owned module area for provider identity and registration types.
- Add adapter-owned descriptor metadata keyed by `ProviderKind`.
- Add runtime-owned instance config keyed by `ProviderInstanceId`.
- Add a runtime-owned registration record that resolves
  `ProviderInstanceId -> ProviderKind + ProviderConfig`.

## File-Sized Steps

1. Add the new core request/execution types in runtime:
   `TaskRequest`, `ResponseMode`, `TransportOptions`, and `ExecutionOptions`.
2. Add the identity types in runtime or a shared runtime-facing types module:
   `ProviderFamilyId`, `ProviderKind`, and `ProviderInstanceId`.
3. Add `RegisteredProvider` and update registration code so routes and runtime
   resolve provider instances separately from adapter kinds.
4. Add `ProviderDescriptor` as adapter-owned static metadata and move adapter
   defaults that belong there out of ad hoc methods.
5. Update provider registration/config code so runtime stores
   `ProviderConfig` by `ProviderInstanceId` instead of using provider kind as
   the only key.
6. Define the composition contract
   `ProviderDescriptor + ProviderConfig -> PlatformConfig` as runtime-owned.
7. Add conversions from current user-facing inputs into `TaskRequest` only,
   without target or response-mode ownership leaking back in.
8. Mark `SendOptions` as migration-only and stop adding new responsibilities to
   it.
9. Make the replacement boundary explicit in public/runtime-facing APIs:
   semantic request input becomes `TaskRequest`, not a renamed monolithic
   replacement for `Request`.

## Locked Ownership Rules To Encode

- `TaskRequest` owns messages, tools, tool choice, response format, shared
  generation controls, stop sequences, and provider payload metadata.
- `TaskRequest` must not own provider instance selection, model selection,
  fallback behavior, streaming mode, transport overrides, or observer
  overrides.
- `TaskRequest` replaces the semantic role of `Request`; `model_id`, `stream`,
  and layered native options must be removed from that semantic request
  surface, not renamed and preserved.
- `TaskRequest.max_output_tokens` is the canonical shared output-token limit
  surface; provider-native options must not add alias fields for the same
  semantic control.
- `ExecutionOptions` owns only `ResponseMode`, observer override, and route-wide
  transport options.
- `ResponseMode` is route-wide and cannot vary per attempt.
- `ResponseMode::NonStreaming` is the baseline execution contract;
  `ResponseMode::Streaming` is the optional streaming contract. Detailed
  fallback-commit and finalization behavior is locked here conceptually and is
  implemented in phases 6 and 7.
- `TransportOptions` is the public typed route-wide transport control surface;
  no generic transport metadata map remains as the primary API.
- `TransportOptions.request_id_header_override` changes response request-id
  extraction only and must not materialize an outbound request header.
- attempt-local transport concerns, including timeout overrides, do not belong
  on `ExecutionOptions.transport`; they land with attempt execution controls in
  phase 2.
- `ProviderFamilyId` selects shared protocol-family behavior.
- `ProviderKind` selects concrete adapter and overlay behavior.
- `ProviderInstanceId` selects one registered runtime destination and its
  config.
- `RegisteredProvider` is the runtime registration record that ties one
  `ProviderInstanceId` to one `ProviderKind` plus runtime-owned config.
- routes must ultimately target `ProviderInstanceId`, not `ProviderKind`, even
  if a temporary migration shim exists before phase 2 fully lands.
- `MessageCreateInput` remains an ergonomic task builder and must normalize into
  `TaskRequest`, not grow provider-specific fields.
- native-option convenience belongs on concrete client APIs or later routing
  surfaces, not on `MessageCreateInput`.
- provider payload metadata belongs on `TaskRequest` / `MessageCreateInput`, not
  on `Route` or `ExecutionOptions`.
- no new canonical replacement `Request` type may be introduced; the target
  public model is `TaskRequest + Route + ExecutionOptions`.

## Repo-Structure Guidance

- Keep public model types separate from planner-internal resolved types.
- Avoid putting new tests inline in implementation files; add dedicated test
  files when this phase lands.
- Do not bury identity types under transport or provider modules; they are
  runtime-wide concepts.
- When a locked behavior is implemented in a later phase, keep the public type
  and ownership boundary here explicit so later phases fill in behavior rather
  than redefine ownership.

## Exit Criteria

- `TaskRequest`, `ResponseMode`, `TransportOptions`, and `ExecutionOptions`
  exist as first-class types with the ownership boundaries from the spec.
- the new semantic request surface does not carry `model_id`, `stream`, or
  layered native options.
- `MessageCreateInput` has a defined normalization path into `TaskRequest`
  without reintroducing target or execution ownership.
- `ProviderFamilyId`, `ProviderKind`, and `ProviderInstanceId` exist and are
  used in the type system.
- `RegisteredProvider` exists or an equivalently explicit runtime registration
  record exists and separates instance identity from adapter identity.
- runtime has a clear descriptor/config/platform split.
- runtime registration and lookup are instance-scoped first, with adapter
  selection happening by resolved `ProviderKind`.
- new code stops expanding the old `Request`/`SendOptions` conceptual model.
- no new monolithic replacement `Request` surface is introduced while this
  phase lands.
