# Phase 2: Routing, Attempts, and Native Options

## Goal

Introduce the routing-layer types that sit beside the new task model:

- `Target`
- `AttemptExecutionOptions`
- `AttemptSpec`
- `Route`
- `NativeOptions`
- `PlanningRejectionPolicy`

This phase makes the routing layer own target ordering and target-scoped native
controls, matching the spec rule that native options are attempt-local and must
not live on `TaskRequest` or `ExecutionOptions`.

## Status

Phase 2 is implemented in the current tree.

The shipped phase-2 slice now includes:

- dedicated route-attempt planning in [crates/agent-runtime/src/route_planning.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/route_planning.rs)
- typed attempt-local native-option wiring through the built-in adapters in [crates/agent-providers/src/adapter.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/adapter.rs)
- planning-time rejection for static incompatibilities, deterministic adapter-planning rejection, and unsupported `ResponseMode::Streaming`
- the explicit `REFACTOR-SHIM:` transport metadata bridge in [crates/agent-runtime/src/provider_runtime/attempt.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime/attempt.rs)

Remaining planner/runtime/transport consolidation work belongs to later phases,
not to the phase-2 routing boundary itself.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- routing layer ownership
- `Target`
- `AttemptExecutionOptions`
- `AttemptSpec`
- `Route`
- `NativeOptions`, `FamilyOptions`, `ProviderOptions`
- `TransportTimeoutOverrides`
- `PlanningRejectionPolicy`
- strict native-option compatibility validation
- model resolution precedence on the attempt target path
- `Route` ownership and exclusion boundaries
- attempt-local transport ownership boundaries and exclusions
- planning-rejection scope for static incompatibilities and deterministic
  adapter-planning rejections
- classification rules for `FamilyOptions` vs `ProviderOptions`

## Current Repo Anchors

- [crates/agent-runtime/src/target.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/target.rs)
- [crates/agent-runtime/src/attempt_execution_options.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/attempt_execution_options.rs)
- [crates/agent-runtime/src/attempt_spec.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/attempt_spec.rs)
- [crates/agent-runtime/src/fallback.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/fallback.rs)
- [crates/agent-runtime/src/planning_rejection_policy.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/planning_rejection_policy.rs)
- [crates/agent-runtime/src/route.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/route.rs)
- [crates/agent-runtime/src/route_planning.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/route_planning.rs)
- [crates/agent-providers/src/platform/openrouter/request.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/platform/openrouter/request.rs)
- [crates/agent-providers/src/openai_family/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/openai_family/mod.rs)
- [crates/agent-providers/src/anthropic_family/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/anthropic_family/mod.rs)
- [crates/agent-core/src/types/native_options.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-core/src/types/native_options.rs)

## Planned Additions

- Replace provider-kind-only `Target` with instance-scoped `Target`.
- Add attempt-local execution overrides that contain only native options,
  timeout overrides, and extra headers.
- Add a first-class `Route` with one primary attempt and ordered fallback
  attempts.
- Add a public typed native-options model that mirrors the family codec plus
  provider overlay split.
- Lock the route-layer ownership boundary so attempt-local overrides stay
  separate from route-wide execution options and adapter protocol hints.
- Add route-layer builder and validation helpers that make correct attempt-local
  composition easier than ad hoc metadata-style overrides.
- Add explicit ownership and merge rules for route-wide headers vs
  attempt-local headers vs adapter-generated provider headers.
- Add dedicated coverage for native-option classification, model-resolution
  precedence on the route path, and planning-rejection-policy behavior.

## File-Sized Steps

1. Replace `Target { provider, model }` with
   `Target { instance, model }`.
2. Add `TransportTimeoutOverrides` as the public helper for attempt-local
   timeout overrides.
3. Add `AttemptExecutionOptions` with only:
   `native`, `timeout_overrides`, and `extra_headers`.
4. Add `AttemptSpec` as the pair of `Target` and `AttemptExecutionOptions`.
5. Add `Route` as `primary + fallbacks + fallback_policy +
   planning_rejection_policy`.
6. Add `FamilyOptions`, `ProviderOptions`, and `NativeOptions`.
7. Move any current provider-specific override channels into the new typed
   native-options surface.
8. Add validation helpers that compare `NativeOptions` variants against the
   resolved target family and provider kind.
9. Encode the `AttemptExecutionOptions` exclusion rules so route-wide transport
   settings, response mode, observer override, and adapter protocol hints cannot
   leak into attempt-local execution controls.
10. Encode `PlanningRejectionPolicy` coverage for static incompatibilities and
    deterministic adapter-planning rejections without introducing broad
    provider capability inference.
11. Add the route-layer header and timeout merge contract so runtime can later
    normalize into `ResolvedTransportOptions` without reopening ownership
    questions in phase 3 or phase 5.
12. Add dedicated tests for target-scoped native options, attempt-local timeout
    overrides, header merge precedence, and planning-skip behavior.

## Locked Ownership Rules To Encode

- `Target.model` is higher precedence than `ProviderConfig.default_model`.
- If neither `Target.model` nor `ProviderConfig.default_model` is present,
  planning must fail before adapter planning or transport execution.
- `Route` owns primary target, ordered fallback targets, fallback decision
  policy, and planning rejection policy.
- `Route` must not own semantic task content, response mode, observer
  overrides, or route-wide transport execution settings.
- Runtime must not rely on provider-side implicit default models.
- Attempt-local native options must never propagate across attempts.
- Mismatched native options are static incompatibilities, not ignored fields.
- `NativeOptions` is layered:
  `family` is consumed by the family codec and `provider` is consumed by the
  provider overlay.
- Timeout overrides here are caller-supplied overrides, not resolved transport
  values.
- `AttemptExecutionOptions` must not own response mode, observer override,
  request-id extraction override, route-wide transport options, or adapter
  protocol hints.
- `AttemptExecutionOptions` may own only:
  family-scoped native options, provider-scoped native options, attempt-local
  timeout overrides, and attempt-local extra headers.
- `Route` is an ordered attempt chain expressed as one primary `AttemptSpec`
  plus ordered fallback `AttemptSpec`s.
- `AttemptSpec` is a routing-layer type only and must not cross the adapter
  boundary.
- `PlanningRejectionPolicy` covers pre-execution planning rejection only:
  static incompatibilities, unsupported streaming capability, unsupported
  native-options layers, and deterministic `ProviderAdapter::plan_request`
  rejections.
- `PlanningRejectionPolicy::FailFast` is the default behavior.
- `PlanningRejectionPolicy` must not become a broad provider capability
  inference mechanism for request features such as tools, structured output,
  `top_p`, stop sequences, reasoning controls, or arbitrary passthrough
  fields.
- `SkipRejectedTargets` records a skipped attempt and advances only during
  planning; it does not convert planning rejection into executed fallback.
- A field belongs in `FamilyOptions` only when its meaning, validation, and
  encoding are shared at the provider-family codec layer.
- A field belongs in `ProviderOptions` when it is concrete-provider-specific or
  its family-shared semantics are not yet locked strongly enough to promote.
- `ProviderOptions` must not restate or override semantics already owned by
  `TaskRequest` or `ExecutionOptions`.
- `TransportOptions.request_id_header_override` remains route-wide only and
  changes response request-id extraction only; it must not be copied into
  `AttemptExecutionOptions`.
- Adapter-owned `HttpRequestOptions` must stay closed to timeout, retry,
  header, method, URL, framing, response-mode, and fallback controls.
- Typed timeout fields are runtime-owned only; adapters do not clamp or mutate
  them.
- Header precedence is explicit:
  platform defaults, then route-wide extra headers, then attempt-local extra
  headers, then adapter/provider-generated dynamic headers, then auth headers
  applied by transport.
- `request_id_header_override` is not part of outbound header merge order.
- First-byte timeout remains transport-internal and is governed by
  `stream_idle_timeout`; phase 2 must not introduce a new public/runtime-facing
  first-byte timeout field.

## Validation Rules To Encode

- Native-option compatibility checks must resolve in this order:
  target instance, registered provider, provider kind, provider family, then
  the `NativeOptions` family/provider variants.
- `NativeOptions.family` mismatching the target family is a static
  incompatibility.
- `NativeOptions.provider` mismatching the resolved provider kind is a static
  incompatibility.
- Family-scoped native options requested for a provider that does not support
  family-native options are a static incompatibility.
- Provider-scoped native options requested for a provider that does not support
  provider-native options are a static incompatibility.
- `ResponseMode::Streaming` requested for a provider whose capabilities do not
  support streaming is a planning rejection covered by
  `PlanningRejectionPolicy`, even though `ResponseMode` itself remains route-
  wide.
- Deterministic `ProviderAdapter::plan_request` rejection is a planning
  rejection, not an executed failure.

## Transport Ownership And Merge Rules

- Route-wide `ExecutionOptions.transport` owns only call-wide transport fields,
  including request-id extraction override and call-wide extra headers.
- `AttemptExecutionOptions` owns only attempt-local extra headers and typed
  timeout overrides; it must not absorb route-wide transport state.
- Provider adapters own closed protocol-level `HttpRequestOptions` and
  provider-generated dynamic headers via request planning.
- Runtime must preserve the ownership split instead of flattening these fields
  into one open-ended metadata map.
- Timeout resolution order is provider/runtime defaults first, then
  attempt-local `TransportTimeoutOverrides`.
- No adapter-owned timeout override or timeout clamp step exists in the target
  architecture.

## Repo-Structure Guidance

- Keep route-layer types distinct from planner-layer resolved attempt types.
- Keep family-scoped and provider-scoped options in separate files/modules so
  they are easy to validate and evolve independently.
- Do not encode native options as stringly metadata or provider-local ad hoc
  maps.
- Keep builder conveniences for `AttemptSpec` and `Route` near the route-layer
  types rather than in provider-specific modules.
- Add dedicated tests in standalone test files for route-layer validation and
  native-option classification behavior; do not put them inline in
  implementation files.

## Test Coverage To Add

- tests proving `Target.instance` is the routing identity and that target
  resolution is instance-scoped before provider kind is considered
- tests proving model resolution precedence is `Target.model` then
  `ProviderConfig.default_model`, with planning failure when both are absent
- tests proving attempt-local native options never leak from one `AttemptSpec`
  to another in the same `Route`
- tests proving mismatched `NativeOptions.family` and mismatched
  `NativeOptions.provider` are treated as static incompatibilities rather than
  ignored fields
- tests proving providers that do not support family-native or provider-native
  options reject those layers during planning
- tests proving `PlanningRejectionPolicy::FailFast` stops on the first planning
  rejection and `SkipRejectedTargets` records a skipped attempt then advances
- tests proving deterministic adapter-planning rejection is recorded as a
  planning rejection rather than an executed failure
- tests proving attempt-local timeout overrides affect only request timeout,
  stream-setup timeout, and stream-idle timeout
- tests proving `request_id_header_override` remains route-wide and does not
  become an outbound request header
- tests proving caller-owned header precedence is route-wide extra headers
  before attempt-local extra headers, with adapter/provider-generated headers
  applied later

## Exit Criteria

- routing is representable as ordered `AttemptSpec`s inside `Route`
- `Route` cleanly owns attempt topology without absorbing task semantics or
  route-wide execution controls
- target identity is instance-scoped
- target model resolution is explicit and never falls through to provider-side
  implicit defaults
- attempt-local timeouts and extra headers are modeled separately from
  route-wide transport options
- attempt-local execution controls reject route-wide execution fields and
  adapter-owned protocol hints
- the ownership split between route-wide transport options, attempt-local
  overrides, and adapter-owned protocol hints is explicit enough to carry into
  transport normalization without reopening precedence questions
- native options are typed, layered, and validated against family/provider
  identity
- `FamilyOptions` vs `ProviderOptions` placement rules are explicit enough to
  keep shared semantics out of provider-local ad hoc surfaces
- planning rejection semantics are explicit for static incompatibilities and
  deterministic adapter-planning rejection paths
- the phase defines the test matrix needed to prove planning-time skips,
  mismatch validation, and target-local override isolation before planner work
  begins

The current implementation satisfies these phase-2 exit criteria. Follow-on
phases still replace the temporary transport shim, add planner-owned resolved
attempt types, and finish the full runtime/observability surfaces defined in
later docs.
