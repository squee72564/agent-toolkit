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
- [crates/agent-runtime/src/fallback.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/fallback.rs)
- [crates/agent-providers/src/platform/openrouter/request.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/platform/openrouter/request.rs)
- [crates/agent-providers/src/openai_family/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/openai_family/mod.rs)
- [crates/agent-providers/src/anthropic_family/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/anthropic_family/mod.rs)

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
- Timeout overrides here are caller-supplied overrides, not resolved transport
  values.
- `AttemptExecutionOptions` must not own response mode, observer override,
  request-id extraction override, route-wide transport options, or adapter
  protocol hints.
- `PlanningRejectionPolicy` covers pre-execution planning rejection only:
  static incompatibilities, unsupported streaming capability, unsupported
  native-options layers, and deterministic `ProviderAdapter::plan_request`
  rejections.
- `PlanningRejectionPolicy::FailFast` is the default behavior.
- `PlanningRejectionPolicy` must not become a broad provider capability
  inference mechanism for request features such as tools, structured output,
  `top_p`, stop sequences, reasoning controls, or arbitrary passthrough
  fields.
- A field belongs in `FamilyOptions` only when its meaning, validation, and
  encoding are shared at the provider-family codec layer.
- A field belongs in `ProviderOptions` when it is concrete-provider-specific or
  its family-shared semantics are not yet locked strongly enough to promote.
- `ProviderOptions` must not restate or override semantics already owned by
  `TaskRequest` or `ExecutionOptions`.

## Repo-Structure Guidance

- Keep route-layer types distinct from planner-layer resolved attempt types.
- Keep family-scoped and provider-scoped options in separate files/modules so
  they are easy to validate and evolve independently.
- Do not encode native options as stringly metadata or provider-local ad hoc
  maps.

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
- native options are typed, layered, and validated against family/provider
  identity
- `FamilyOptions` vs `ProviderOptions` placement rules are explicit enough to
  keep shared semantics out of provider-local ad hoc surfaces
- planning rejection semantics are explicit for static incompatibilities and
  deterministic adapter-planning rejection paths
