# Phase 6: Runtime Orchestration, Fallback, and Route Failures

## Goal

Refactor runtime orchestration so routed execution uses ordered attempts from
`Route`, planning-time skips remain distinct from executed failures, and
fallback is rule-driven rather than target-owned.

This phase is about orchestration behavior, not re-defining the public
planning-failure, metadata, or streaming terminal shapes that phases 3, 7, and
9 own. Phase 6 must wire those locked contracts together correctly at runtime.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- `FallbackPolicy`, `FallbackRule`, `FallbackMatch`, `FallbackAction`
- removal of fallback targets from `FallbackPolicy`
- insertion-ordered rule evaluation
- AND semantics across non-empty match fields
- field-specific fallback matching for `error_kind`, status code, provider code,
  provider kind, and provider instance
- status-code matcher failure when the normalized error has no status code
- provider-code trimming and blank provider-code non-matches
- default-policy behavior where fallback does not continue unless explicit
  rules match
- fallback only after normalized executed failures
- fallback never against raw transport responses or planning-only outcomes
- skipped attempts and planning failures
- runtime orchestration of `PlanningRejectionPolicy`
- skip-path observer orchestration via `on_attempt_skipped` only
- route-owned ordered attempt iteration
- separation of transport retry from route fallback

Streaming-specific commit behavior is owned by phase 7, but phase 6 must keep
non-streaming/runtime orchestration ready for the same fallback contract.

## Current Repo Anchors

- [crates/agent-runtime/src/fallback.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/fallback.rs)
- [crates/agent-runtime/src/agent_toolkit/execution.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/agent_toolkit/execution.rs)
- [crates/agent-runtime/src/provider_runtime.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime.rs)

## Planned Additions

- Make `Route` own ordered attempts.
- Reduce `FallbackPolicy` to rule evaluation only.
- Add or refactor runtime attempt-cursor logic that consumes ordered
  `AttemptSpec`s from `Route`.
- Ensure runtime orchestration consumes the planner's skipped-attempt and
  route-planning-failure outputs without reclassifying them as executed
  failures.
- Ensure fallback evaluation runs only on normalized `RuntimeError` values
  produced after executed failures.
- Ensure runtime consumes decoded error details after family-codec decoding and
  provider-overlay refinement before fallback-rule evaluation.

## File-Sized Steps

1. Remove `targets` ownership from `FallbackPolicy`.
2. Remove legacy fallback toggles from the target architecture and keep any
   temporary normalization behind explicit `REFACTOR-SHIM:` markers only if
   strictly needed.
3. Update runtime orchestration to iterate route attempts strictly in route
   order and to advance only through planner-directed skip behavior governed by
   `PlanningRejectionPolicy` or executed-failure fallback behavior governed by
   `FallbackPolicy`.
4. Update runtime orchestration so planning-time rejections remain planner
   outputs, never invoke fallback, and either fail fast or skip-and-advance
   according to `PlanningRejectionPolicy`.
5. Update executed-failure handling so fallback is evaluated only after runtime
   has normalized an executed failure into `RuntimeError`, including
   family-codec and provider-overlay error decoding when available.
6. Update rule matching so fallback may match by error kind, status code,
   provider code, provider kind, and provider instance, with insertion order
   and AND semantics across all non-empty matcher fields.
7. Ensure status-code matchers fail when the normalized executed error has no
   status code, and ensure provider-code rule evaluation trims surrounding
   whitespace and that blank rule values never match.
8. Ensure the default policy is exactly an empty rules list and does not
   continue fallback unless an explicit rule matches.
9. Ensure `RetryNextTarget` advances only to the next attempt already present
   on `Route`, while `Stop` or no matching rule surfaces the current executed
   failure.
10. Ensure planning-time skip orchestration preserves the phase-9 observer
    contract by emitting `on_attempt_skipped` only, never
    `on_attempt_start` / `on_attempt_failure`, for planning-only rejections.
11. Ensure skip paths and route-planning failures remain distinct from executed
   failure handling and never emit fallback evaluation.
12. Keep transport retry intra-attempt only and ensure route orchestration does
   not treat transport retries as new route attempts.

## Locked Rules To Encode

- `FallbackPolicy::default()` is exactly an empty rules list in the target
  architecture
- the default policy performs no fallback retries on its own
- first matching fallback rule wins
- no matching rule stops fallback
- status-code matching fails when the normalized error has no status code
- blank provider code values do not match
- provider-code matching happens only after normalized error decoding populated
  a provider code
- provider-overlay decoded error fields take precedence over family-decoded
  fields before fallback matching
- planning rejection does not invoke fallback
- `RetryNextTarget` advances only to the next attempt already present on
  `Route`
- `FallbackAction::Stop` surfaces the current executed failure without route
  advancement
- fallback is evaluated only against normalized executed failures, not raw
  transport responses
- transport retry is not route fallback

## Repo-Structure Guidance

- keep fallback rule types separate from attempt-history types
- keep planning-failure types separate from executed runtime errors
- avoid mixing skip-path and execution-path metadata in the same structs unless
  the spec explicitly requires it
- depend on the public shapes introduced in phases 3 and 9 instead of
  re-defining alternate route-failure or attempt-history models here

## Exit Criteria

- route fallback depends only on rule evaluation plus route ordering
- runtime honors `PlanningRejectionPolicy` without reclassifying planning
  outcomes as executed failures
- fallback evaluates only normalized executed failures and respects the locked
  matcher semantics
- planning-only skip orchestration preserves `on_attempt_skipped` behavior
  without execution-start/failure observer events
- skipped attempts and executed failures are represented separately
- route planning failures are first-class outputs
- transport retry remains intra-attempt behavior rather than route advancement
