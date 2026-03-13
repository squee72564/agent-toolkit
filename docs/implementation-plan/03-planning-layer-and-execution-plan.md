# Phase 3: Planning Layer and ExecutionPlan

## Goal

Create the planning layer that resolves:

- `TaskRequest`
- `Route`
- `ExecutionOptions`

into a fully resolved attempt contract:

- `ResolvedProviderAttempt`
- `ExecutionPlan`

This phase is the center of the refactor. It removes the current spread-out
planning logic and makes planning-time rejection, compatibility validation,
model resolution, and transport normalization explicit.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- planning layer ownership
- `ExecutionPlan`
- `ResolvedProviderAttempt`
- model resolution rules
- static capability mismatch
- static capability scope limits
- adapter-planning rejection
- planning rejection policy behavior
- route planning failures before execution
- resolved transport option construction

## Current Repo Anchors

- [crates/agent-runtime/src/provider_runtime.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime.rs)
- [crates/agent-runtime/src/provider_runtime/attempt.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime/attempt.rs)
- [crates/agent-runtime/src/agent_toolkit/execution.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/agent_toolkit/execution.rs)
- [crates/agent-runtime/src/provider_client.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_client.rs)

## Planned Additions

- Add a dedicated planner module in runtime.
- Add resolved provider attempt types separate from route-layer types.
- Add `ExecutionPlan` as the only adapter-facing attempt planning input.
- Add planning-time result types that distinguish skipped attempts from
  executed failures.
- Add planner-owned resolution for registered provider, platform config, and
  auth context before adapter planning begins.

## File-Sized Steps

1. Add planner input and result modules under runtime.
2. Centralize registered-provider resolution from route target to concrete
   runtime destination before adapter planning.
3. Add `ResolvedProviderAttempt` carrying:
   `instance_id`, `provider_kind`, `family`, resolved model, capabilities
   context, and native options.
4. Add `ExecutionPlan` carrying:
   response mode, `task`, resolved attempt, resolved platform, auth context,
   resolved capabilities, and resolved transport options.
5. Centralize model resolution:
   `Target.model` first, then `ProviderConfig.default_model`, else planning
   failure with no provider-side implicit defaulting.
6. Centralize `PlatformConfig`, auth-context, and transport-option resolution
   before adapter request planning.
7. Centralize static incompatibility checks for:
   streaming support, family-native-option support, provider-native-option
   support, and concrete native-option family/provider mismatch.
8. Add adapter-planning rejection handling as a distinct planning outcome.
9. Encode `PlanningRejectionPolicy::FailFast` as immediate planning failure and
   `PlanningRejectionPolicy::SkipRejectedTargets` as ordered skip-and-advance
   behavior only for pre-execution planning rejections.
10. Add `RoutePlanningFailure` production when no compatible/executable attempt
   exists before any attempt executes, including
   `NoCompatibleAttempts` vs `AllAttemptsRejectedDuringPlanning`.
11. Remove target/model/native-option resolution logic from ad hoc runtime paths
   that currently prepare attempts directly.

## Locked Rules To Encode

- `AttemptSpec` is a routing-layer type only and must not cross the adapter
  boundary.
- `ExecutionPlan` is the single resolved-attempt contract.
- Runtime must resolve model selection before `ExecutionPlan` creation and must
  not defer model choice to provider-side implicit defaults.
- Runtime/provider-instance configuration is consumed before `ExecutionPlan` is
  created; adapters receive resolved provider state, not unresolved config.
- Static incompatibility is intentionally narrow and limited to:
  streaming support, family-native-option support, provider-native-option
  support, and native-option family/provider identity mismatch.
- Features whose support can vary by model or deployment, such as tools,
  structured output, `top_p`, stop sequences, reasoning controls, and
  passthrough fields, are not part of static capability validation.
- Adapter-planning rejection is not an executed failure.
- Adapter-planning rejection and static incompatibility never enter executed
  fallback evaluation.
- `PlanningRejectionPolicy::FailFast` stops routing on the current planning
  rejection; `SkipRejectedTargets` records a skipped attempt and advances only
  before execution.
- `RoutePlanningFailure` must carry ordered `AttemptRecord` history with the
  same resolved model-selection rule used by `ResolvedProviderAttempt.model`,
  must use the same `AttemptRecord` shape, ordering rules, and skip semantics
  as other route-attempt metadata surfaces,
  must not create an `AttemptRecord` when model resolution fails before a
  candidate attempt is concretely resolved,
  and skipped attempts must not contain provider request id or executed-attempt
  status metadata,
  but must not carry success-only response metadata or executed-failure
  metadata, and it is produced only when routing ends before any attempt yields
  success or an executed failure.

## Repo-Structure Guidance

- Keep planner code in runtime, not in adapters and not in transport.
- Keep route-layer inputs, resolved planner state, and transport-facing state in
  different modules.
- Add dedicated tests for model resolution and planning rejection behavior as
  standalone test files.
- Add dedicated tests for `PlanningRejectionPolicy::FailFast` and
  `PlanningRejectionPolicy::SkipRejectedTargets`, including ordered skipped
  attempt history and `RoutePlanningFailure.reason`.
- Add dedicated tests proving planning-only skipped attempts never receive
  provider request id or executed-attempt status metadata, and that missing
  model resolution fails before an `AttemptRecord` is emitted.

## Exit Criteria

- one runtime planner owns attempt resolution and `ExecutionPlan` creation
- adapters no longer need unresolved route-layer data
- provider-instance, platform, auth, and transport resolution are runtime-owned
  planner responsibilities before adapter request planning
- planning failures, skipped attempts, executable attempts, and executed
  failures are separate typed outcomes
- static capability validation is limited to the locked high-confidence
  invariants and does not expand into model-level feature inference
- route-planning failure preserves ordered skipped-attempt history with the
  correct failure reason and without executed-failure metadata
- planning-only `AttemptRecord` emission follows the locked route-failure
  semantics for resolved model recording and absence of execution-only metadata

## Execution Metadata Boundaries

### Phase 03 Scope

Phase 03 owns planning-layer metadata only:
- `RoutePlanningFailure.attempts: Vec<AttemptRecord>` carries planning-only skip history
- Skipped attempts never contain provider request-id or executed-attempt status metadata
- Pre-execution planning rejection records include resolved provider identity and model

### Deferred to Phase 06/09

Execution-phase attempt history tracking is intentionally out of scope for Phase 03:
- `ResponseMeta.attempts` continues using `Vec<AttemptMeta>` (execution summary)
- `ExecutedFailureMeta` creation and unified attempt history (planning + execution phases)
- Fallback-time planning rejections are not yet recorded in execution-phase history
- Observer event `on_attempt_skipped` for execution-time planning rejections
- Unified `AttemptRecord` usage across all success/failure metadata surfaces

**Rationale**: Phase 06 (Runtime Orchestration) owns attempt ordering, emission timing, and execution-phase history accumulation patterns required for full unification. Phase 09 (Observability, Metadata, and Errors) implements the unified metadata model once orchestration patterns are established.

### Current Behavior

**Planning-phase failures** (`RoutePlanningFailure`):
- Records all planning rejections as `AttemptRecord` with `AttemptDisposition::Skipped`
- Distinguishes `NoCompatibleAttempts` vs `AllAttemptsRejectedDuringPlanning`
- Includes resolved provider instance, provider kind, and effective model

**Execution-phase successes** (`ResponseMeta`):
- Uses `Vec<AttemptMeta>` for executed attempt history
- Streaming path accumulates all executed fallback attempts
- Non-streaming path tracks only the final successful attempt

**Execution-phase failures** (current RuntimeError):
- Non-streaming: Only final error tracked, not intermediate fallback attempts
- Streaming: Accumulates executed attempts but drops planning rejections during fallback
- No `ExecutedFailureMeta` surface exists yet

This boundary is intentional and will be resolved when Phase 06 establishes runtime orchestration patterns and Phase 09 implements the unified metadata model.
