# Phase 9: Observability, Metadata, and Errors

## Goal

Align observer events, attempt history, response metadata, and failure surfaces
with the route-oriented attempt model.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- attempt history metadata
- `AttemptRecord`
- `AttemptDisposition`
- `SkipReason`
- `RoutePlanningFailure`
- `RoutePlanningFailureReason`
- `ResponseMeta`
- `ExecutedFailureMeta`
- executed failure metadata
- observer event behavior for skipped vs executed attempts
- selected-attempt metadata fields
- provider kind and provider instance inclusion in returned metadata
- ordered attempt indices and effective-model recording
- streaming terminal metadata/failure surfaces

## Current Repo Anchors

- [crates/agent-runtime/src/observer.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/observer.rs)
- [crates/agent-runtime/src/runtime_error.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/runtime_error.rs)
- [crates/agent-runtime/src/types.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/types.rs)
- [crates/agent-runtime/src/provider_runtime.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime.rs)

## Planned Additions

- Add spec-aligned attempt metadata shapes.
- Add route-planning failure outputs distinct from executed runtime errors.
- Update observer callbacks so skipped attempts have dedicated events.
- Ensure attempt history is ordered and shared across success and planning
  failure paths.
- Keep incremental streaming events free of partial/live route metadata while
  preserving terminal success/failure metadata outputs.

## File-Sized Steps

1. Add or reshape attempt metadata types to carry:
   provider instance, provider kind, effective model, `target_index`,
   `attempt_index`, and a typed disposition that distinguishes skipped,
   succeeded, and executed-failed attempts, with normalized error kind/message
   for executed failures and status code/request id fields present only for
   executed attempts when available.
2. Add `RoutePlanningFailure` and `RoutePlanningFailureReason`, with explicit
   `NoCompatibleAttempts` vs `AllAttemptsRejectedDuringPlanning` behavior, and
   ensure this surface is returned only when routing ends before any executed
   attempt succeeds or fails in the normal execution sense.
3. Update `ResponseMeta` and `ExecutedFailureMeta` so terminal success and
   terminal executed failure each carry:
   selected provider instance, selected provider kind, selected effective
   model, final status code/request id when available, and the full ordered
   `AttemptRecord` history using the same `AttemptRecord` shape, ordering
   rules, and skip semantics as the other route metadata surfaces.
4. Update observer APIs so:
   skipped attempts emit `on_attempt_skipped`
   and do not emit `on_attempt_start` or `on_attempt_failure`, while
   `on_attempt_start`, `on_attempt_success`, and `on_attempt_failure` remain
   execution-only lifecycle callbacks.
5. Ensure `on_attempt_skipped` is emitted only after runtime has resolved
   concrete attempt identity and effective model, and that the event carries
   the same provider instance, provider kind, model, `target_index`, and
   `attempt_index` recorded in the corresponding skipped `AttemptRecord`, plus
   elapsed planning time for that skipped attempt.
6. Update error normalization so executed failures still produce `RuntimeError`
   plus `ExecutedFailureMeta`, while planning-only failure paths produce
   `RoutePlanningFailure`.
7. Ensure stream events themselves remain metadata-light, terminal streaming
   success yields a completed canonical response with normal `ResponseMeta`,
   and terminal streaming executed failure yields normalized `RuntimeError`
   plus `ExecutedFailureMeta`.
8. Encode the model-resolution rule in metadata production: if runtime cannot
   resolve an effective model for a candidate attempt, planning fails before an
   `AttemptRecord` or skip event is emitted for that attempt.

## Locked Rules To Encode

- returned attempt history includes skipped and executed attempts
- attempt records include provider instance and provider kind
- attempt records include resolved model, `target_index`, and `attempt_index`
- skipped attempts never carry provider request id or executed-attempt status
  metadata
- executed-failed attempt records carry normalized failure kind/message plus
  final status code/request id only when available after execution began
- `ResponseMeta.selected_model` is always the concrete resolved model of the
  successful executed attempt
- `ResponseMeta.status_code` and `request_id` follow the successful executed
  attempt when available
- `ResponseMeta.attempts`, `ExecutedFailureMeta.attempts`, and
  `RoutePlanningFailure.attempts` all use the same `AttemptRecord` shape,
  ordering rules, and skip semantics
- `ExecutedFailureMeta` identifies the concrete executed failed attempt and is
  never used for planning-only outcomes
- `ExecutedFailureMeta.status_code` and `request_id` follow the final executed
  failed attempt when available
- route planning failure must not invent success-only fields
- route planning failure reasons must preserve the
  `NoCompatibleAttempts` / `AllAttemptsRejectedDuringPlanning` distinction
- executed failures return normalized `RuntimeError`, not `RoutePlanningFailure`
- skipped attempts emit only `on_attempt_skipped`, never execution lifecycle
  callbacks
- `on_attempt_success` and `on_attempt_failure` apply only to executed
  attempts after provider execution began
- incremental stream events never expose partial/live `ResponseMeta`

## Repo-Structure Guidance

- keep metadata structs small and purpose-built
- avoid a single mega-struct that tries to represent both planning failures and
  executed successes
- add dedicated tests for observer event ordering and metadata contents
- keep success metadata, executed-failure metadata, and planning-failure
  metadata as separate public surfaces even if they share `AttemptRecord`

## Exit Criteria

- response and failure metadata align with the route-attempt model
- observer semantics distinguish skipped attempts from executed attempts
- planning failures and executed failures are separate public surfaces
- terminal streaming success/failure uses the same locked metadata/error
  surfaces as non-streaming execution
