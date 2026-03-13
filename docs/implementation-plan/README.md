# Multi-Provider Refactor Implementation Plan

This doc set is the implementation plan for the architecture defined in
[docs/REFACTOR.md](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/docs/REFACTOR.md).
[docs/SPEC-WALKTHROUGH.md](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/docs/SPEC-WALKTHROUGH.md)
is supporting material only.

## Purpose

Use this plan to implement the runtime redesign without drifting from the
locked ownership boundaries in `REFACTOR.md`.

The plan is organized by architectural boundary, not by crate alone and not by
"all types first, all wiring later." Each phase is a dependency-ordered,
file-sized slice with clear exit criteria.

## Deliverables

- [Phase 1: Core Model and Identity](./01-core-model-and-identity.md)
- [Phase 2: Routing, Attempts, and Native Options](./02-routing-attempts-and-native-options.md)
- [Phase 3: Planning Layer and ExecutionPlan](./03-planning-layer-and-execution-plan.md)
- [Phase 4: Adapter Boundary Redesign](./04-adapter-boundary-redesign.md)
- [Phase 5: Transport Boundary Redesign](./05-transport-boundary-redesign.md)
- [Phase 6: Runtime Orchestration, Fallback, and Route Failures](./06-runtime-orchestration-fallback-and-failures.md)
- [Phase 7: Streaming Commit and Finalization](./07-streaming-commit-and-finalization.md)
- [Phase 8: Public API and Client Migration](./08-public-api-and-client-migration.md)
- [Phase 9: Observability, Metadata, and Errors](./09-observability-metadata-and-errors.md)
- [Phase 10: Tests, Examples, and Shim Removal](./10-tests-examples-and-shim-removal.md)
- [Appendix A: Old-to-New Mapping](./appendix-a-old-to-new-mapping.md)
- [Appendix B: Locked Decisions Checklist](./appendix-b-locked-decisions-checklist.md)

## Current Repo Anchors

The largest current migration surfaces are:

- [crates/agent-runtime/src/send_options.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/send_options.rs)
- [crates/agent-runtime/src/target.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/target.rs)
- [crates/agent-runtime/src/message_create_input.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/message_create_input.rs)
- [crates/agent-runtime/src/provider_runtime.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime.rs)
- [crates/agent-runtime/src/provider_runtime/attempt.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime/attempt.rs)
- [crates/agent-runtime/src/provider_runtime/transport.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime/transport.rs)
- [crates/agent-runtime/src/fallback.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/fallback.rs)
- [crates/agent-runtime/src/direct_messages_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/direct_messages_api.rs)
- [crates/agent-runtime/src/direct_streaming_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/direct_streaming_api.rs)
- [crates/agent-runtime/src/routed_messages_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/routed_messages_api.rs)
- [crates/agent-runtime/src/routed_streaming_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/routed_streaming_api.rs)
- [crates/agent-runtime/src/observer.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/observer.rs)
- [crates/agent-runtime/src/runtime_error.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/runtime_error.rs)
- [crates/agent-runtime/src/types.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/types.rs)
- [crates/agent-providers/src/adapter.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/adapter.rs)
- [crates/agent-transport/src/http/request.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-transport/src/http/request.rs)

## Phase Order

1. Core model and identity
2. Routing, attempts, and native options
3. Planning layer and `ExecutionPlan`
4. Adapter boundary redesign
5. Transport boundary redesign
6. Runtime orchestration, fallback, and route failures
7. Streaming commit and finalization
8. Public API and client migration
9. Observability, metadata, and errors
10. Tests, examples, and shim removal

## Dependency Rules

- Do not implement phase 4 before phase 3 has locked the planner and
  `ExecutionPlan` boundary.
- Do not implement phase 5 before phase 4 has locked adapter-owned request
  outputs.
- Do not implement phase 8 before phases 1 through 7 have stabilized the
  internal model.
- Do not declare the refactor complete until appendix B is fully checked off.

## Working Style

- Keep edits file-sized and digestible.
- Prefer introducing new files/modules for new boundaries rather than
  overloading existing catch-all modules.
- Favor target architecture over compatibility scaffolding.
- Allow only minimal temporary shims.
- Mark every temporary shim with `REFACTOR-SHIM:` so it can be found and
  removed in phase 10.
- Treat failing intermediate builds as acceptable if they are in service of
  landing a clean phase boundary and if the next step resolves them quickly.

## Phase Coverage Map

- Phase 1 covers the task/execution split, identity split, descriptor/config
  ownership, the registered-provider model, retirement of monolithic
  `Request` ownership, and the public core model including
  `MessageCreateInput -> TaskRequest` normalization and route-wide
  `ExecutionOptions` / `TransportOptions` ownership.
- Phase 2 covers routing-layer types, instance-scoped targets and model
  selection on the attempt path, attempt-local execution controls and their
  ownership boundaries, `Route` ownership boundaries, layered native options
  including family-vs-provider classification rules, and planning-rejection
  policy for static incompatibilities and deterministic adapter-planning
  rejection.
- Phase 3 covers planning, compatibility checks, resolved attempts, and
  `ExecutionPlan`, including the locked model-resolution precedence, narrow
  static-capability validation scope, planning-rejection-policy behavior, and
  pre-execution `RoutePlanningFailure` outcomes.
- Phase 4 covers provider descriptors, adapter composition, family codecs, and
  provider overlays, including the full adapter-owned `ProviderRequestPlan`
  boundary (`method`, `body`, `endpoint_path_override`, `provider_headers`,
  `HttpRequestOptions`, and `response_framing`), runtime-owned
  `ProviderDescriptor` + `ProviderConfig` -> `PlatformConfig` composition, and
  adapter decode/projector orchestration where family-codec defaults are
  refined by provider overlays, plus the `EncodedFamilyRequest` intermediate
  and adapter-exposed static metadata surfaces (`ProviderDescriptor` and
  `ProviderCapabilities`).
- Phase 5 covers the typed runtime/provider/transport boundary, including
  `TransportExecutionInput`, `ResolvedTransportOptions`, adapter-owned method
  and transport-framing selection, runtime-owned URL construction and
  transport-input normalization, timeout/retry/header ownership, response
  framing, request-id extraction semantics, and removal of metadata-driven
  transport control.
- Phase 6 covers runtime orchestration over ordered route attempts, including
  route-owned fallback topology, rule-driven fallback evaluation against
  normalized executed failures only, first-match / AND-match fallback-rule
  behavior, explicit-rule default fallback behavior, provider-kind and
  provider-instance fallback matching, separation of planning-time rejection
  from executed-failure fallback, planning-skip observer orchestration, and
  separation of intra-attempt transport retry from inter-attempt route
  fallback. This phase consumes the planning, metadata, and streaming
  contracts locked in phases 3, 7, and 9 rather than redefining those public
  shapes.
- Phase 7 covers the route-wide `ResponseMode::Streaming` contract, commit at
  first canonical event emission, runtime-owned pre-commit vs post-commit
  streaming failure classification, fallback cutover, no partial/live
  `ResponseMeta` on incremental events, and terminal finalization into either a
  completed canonical `Response` or normalized `RuntimeError` plus
  `ExecutedFailureMeta`.
- Phase 8 covers direct clients, routed clients, builder ergonomics, and
  normalization from `MessageCreateInput`, including the locked
  provider-agnostic `MessageCreateInput` boundary, direct-client generated
  single-attempt routes targeting configured `ProviderInstanceId`s, concrete
  client native-option convenience, routing-only `Route` public semantics,
  inferred default `ExecutionOptions` for ergonomic `.messages()` /
  `.streaming()` entrypoints, explicit routed `ExecutionOptions` overloads,
  replacement of monolithic low-level request entrypoints with `TaskRequest` +
  `Route` + `ExecutionOptions`, and any temporary legacy public-input shims
  needed to normalize old `Request`, `SendOptions`, or provider-kind-targeted
  routes during migration.
- Phase 9 covers observer events, route-attempt metadata, success/failure
  metadata surfaces, and planning-failure surfaces, including the locked
  `AttemptDisposition` / `AttemptRecord` shape (`provider_instance`,
  `provider_kind`, resolved `model`, `target_index`, `attempt_index`,
  status-code/request-id payloads only for executed attempts), exact
  `ResponseMeta` / `ExecutedFailureMeta` selected-attempt fields, ordered
  skipped-plus-executed attempt history, `RoutePlanningFailureReason`
  (`NoCompatibleAttempts` vs `AllAttemptsRejectedDuringPlanning`),
  `on_attempt_skipped` identity/order guarantees and non-emission of execution
  lifecycle callbacks for skipped attempts, and streaming terminal metadata
  semantics where incremental events stay metadata-light while terminal success
  yields `ResponseMeta` and terminal executed failure yields `RuntimeError` plus
  `ExecutedFailureMeta`.
- Phase 10 covers fixture and live test continuity, all required coverage
  additions, example updates, migration cleanup, compatibility-shim removal,
  and final removal of every `REFACTOR-SHIM:` marker.

## Done Criteria

The refactor is done only when:

- all 10 phases have met their exit criteria
- appendix A matches the shipped public and internal model
- appendix B has no unchecked locked decisions
- the implementation-plan phase docs still cover every locked decision they are
  responsible for from `REFACTOR.md`
- every required coverage addition from `REFACTOR.md` is implemented
- no `REFACTOR-SHIM:` markers remain
