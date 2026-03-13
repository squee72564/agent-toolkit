# Phase 10: Tests, Examples, and Shim Removal

## Goal

Complete the refactor by proving the new architecture through tests and by
removing any temporary migration-only code paths.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- fixture and live test continuity
- every required coverage addition listed in `REFACTOR.md`
- migration/compatibility cleanup
- final removal of temporary shims

## Fixture and Live Expectations

`REFACTOR.md` treats existing fixture and live coverage as part of the
architecture contract, not as optional cleanup.

- fixture tests remain authoritative and continue to pass while helpers/builders
  migrate to `TaskRequest`, `Route`, `ExecutionOptions`, and `ExecutionPlan`
- fixture updates preserve real provider payload expectations
- live tests continue to pass
- live tests continue proving direct-client ergonomics, routed fallback,
  family-shared behavior, and correct application of layered attempt-local
  native options

## Compatibility Cleanup Scope

Do not stop at removing comment markers. This phase also removes migration-only
compatibility behavior called out in `REFACTOR.md`, including any remaining:

- old `Request` normalized into `TaskRequest +` single-attempt `Route + ExecutionOptions`
- old `SendOptions` normalized into `Route + ExecutionOptions`
- old provider-kind-targeted routes normalized into instance-targeted routes
  through a temporary runtime lookup shim
- old fallback toggles such as `retry_on_status_codes`,
  `retry_on_transport_error`, and `FallbackMode` normalized into equivalent
  ordered `FallbackRule`s during migration
- adapters temporarily retaining internal helpers equivalent to today's
  `platform_config(base_url)` while runtime-owned `ProviderDescriptor`
  composition is introduced
- the existing transport field/type name `HttpResponseMode` temporarily
  retained during migration even though the target architecture treats that
  concept as transport-level `TransportResponseFraming`

## Current Repo Anchors

- [crates/agent-runtime/src/test/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/test/mod.rs)
- [crates/agent-runtime/tests/observer_test.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/tests/observer_test.rs)
- [crates/agent-providers/tests/provider_contract_test.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/tests/provider_contract_test.rs)
- [crates/agent-transport/tests/http_test.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-transport/tests/http_test.rs)
- [crates/agent/tests/e2e_router_fallback_observability_test.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent/tests/e2e_router_fallback_observability_test.rs)
- [crates/agent/examples/basic_openai.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent/examples/basic_openai.rs)

## Test Matrix

The final implementation must cover every required addition listed in
`REFACTOR.md`. Track them as checkboxes here and close the phase only when all
are done.

- [ ] direct OpenAI request with family-scoped options only
- [ ] direct OpenAI request with both family-scoped and OpenAI-specific options
- [ ] direct OpenRouter request with both OpenAI-family and OpenRouter-specific options
- [ ] routed fallback across two registered self-hosted `GenericOpenAiCompatible` instances
- [ ] routed OpenAI primary plus OpenRouter fallback with different attempt-local layered native options
- [ ] routed mixed-family fallback path
- [ ] tests proving `Target.instance` resolves to the intended registered provider instance before adapter planning
- [ ] tests proving adapter lookup uses `ProviderKind` while config/auth lookup uses `ProviderInstanceId`
- [ ] tests proving returned `ResponseMeta` and `AttemptRecord` values include both provider instance and provider kind
- [ ] tests proving `AttemptRecord.model` stores the concrete effective model for skipped, failed, and successful attempts, including when the effective model came from `ProviderConfig.default_model`
- [ ] tests proving `AttemptRecord` preserves `target_index` and `attempt_index` ordering across skipped and executed attempts
- [ ] tests proving skipped-attempt `AttemptRecord`s never carry status code or request id fields
- [ ] tests proving executed-failed `AttemptRecord`s preserve normalized `RuntimeErrorKind`, failure message, and final status/request-id fields when available
- [ ] tests proving `ResponseMeta.selected_provider_instance`, `selected_provider_kind`, and `selected_model` identify the successful executed attempt
- [ ] tests proving `ResponseMeta.status_code` and `request_id` follow the successful executed attempt when available
- [ ] tests proving `ExecutedFailureMeta.selected_provider_instance`, `selected_provider_kind`, and `selected_model` identify the final executed failed attempt
- [ ] tests proving `ExecutedFailureMeta.status_code` and `request_id` follow the final executed failed attempt when available
- [ ] agent-transport header tests updated to validate typed request-id override and typed extra header inputs instead of AdapterContext.metadata magic keys
- [ ] runtime/provider integration tests proving that adapter-produced HttpRequestOptions merge correctly with route-wide and attempt-local typed transport inputs
- [ ] runtime/provider integration tests proving `ProviderRequestPlan.method` is copied into `TransportExecutionInput.method` without transport-side inference
- [ ] runtime/provider integration tests proving `ProviderRequestPlan.response_framing` is copied into `TransportExecutionInput.response_framing` without transport-side inference
- [ ] runtime/provider integration tests proving runtime rejects framing incompatible with `ExecutionOptions.response_mode`
- [ ] runtime/provider integration tests proving final URL construction uses `ProviderRequestPlan.endpoint_path_override` when present and `ProviderDescriptor.endpoint_path` otherwise
- [ ] runtime/provider integration tests proving `ProviderConfig.base_url` override is joined with the resolved effective endpoint path before transport execution
- [ ] runtime/provider integration tests proving adapter-produced dynamic headers merge in the locked header-precedence order
- [ ] runtime/provider integration tests proving non-success JSON responses are decoded into provider-specific `RuntimeError` values before fallback evaluation when `preserve_error_body_for_adapter_decode` is enabled
- [ ] tests confirming family codec error decoding runs before provider overlay error decoding and that provider-overlay fields take precedence on collision
- [ ] tests confirming `FallbackRule.provider_codes` matches only after family + provider error decoding and runtime normalization have populated a provider code
- [ ] tests confirming `FallbackPolicy.rules` are evaluated in insertion order and first match wins
- [ ] tests confirming `FallbackMatch` uses AND semantics across all non-empty fields
- [ ] tests confirming fallback rules can match `ProviderKind`
- [ ] tests confirming fallback rules can match `ProviderInstanceId`
- [ ] tests confirming `FallbackAction::Stop` prevents fallback
- [ ] tests confirming that no matching fallback rule prevents fallback
- [ ] tests confirming blank `provider_codes` rule values do not match
- [ ] static capability mismatch with FailFast for each locked high-confidence invariant
- [ ] static capability mismatch with SkipRejectedTargets for each locked high-confidence invariant
- [ ] tests confirming that PlanningRejectionPolicy::FailFast does not invoke fallback
- [ ] tests confirming that PlanningRejectionPolicy::SkipRejectedTargets records skipped attempts and continues routing
- [ ] tests confirming adapter-planning rejections are recorded as `SkipReason::AdapterPlanningRejected`
- [ ] tests confirming adapter-planning rejections emit `on_attempt_skipped` and do not emit `on_attempt_start` / `on_attempt_failure`
- [ ] tests confirming adapter-planning rejections do not invoke fallback
- [ ] tests proving `RoutePlanningFailure.attempts` preserves the same ordered `AttemptRecord` contract as `ResponseMeta.attempts`
- [ ] tests proving `ExecutedFailureMeta.attempts` uses the same ordered `AttemptRecord` contract and skip semantics as `ResponseMeta.attempts`
- [ ] tests proving `RoutePlanningFailure.reason = NoCompatibleAttempts` when all remaining attempts are rejected by static compatibility rules
- [ ] tests proving `RoutePlanningFailure.reason = AllAttemptsRejectedDuringPlanning` when at least one attempt reaches adapter planning but all remaining attempts are rejected before execution
- [ ] tests proving route-planning failure does not carry success-only metadata such as selected provider/model or upstream request id
- [ ] tests proving all-planning-rejected routes return `RoutePlanningFailure`, not `RuntimeError`
- [ ] tests proving executed failures return normalized `RuntimeError`, not `RoutePlanningFailure`
- [ ] tests confirming skipped attempts appear in returned `ResponseMeta.attempts`
- [ ] tests confirming skipped attempts emit `on_attempt_skipped` and do not emit `on_attempt_start` / `on_attempt_failure`
- [ ] tests confirming `on_attempt_success` is emitted only for executed successful attempts and `on_attempt_failure` only for executed failed attempts
- [ ] tests confirming `on_attempt_skipped` carries the same provider instance, provider kind, model, `target_index`, and `attempt_index` as the corresponding skipped `AttemptRecord`
- [ ] tests confirming `on_attempt_skipped.elapsed` reflects the skipped attempt's planning-time duration
- [ ] tests proving missing effective-model resolution fails before any `AttemptRecord` or `on_attempt_skipped` event is emitted for that candidate attempt
- [ ] tests proving transport SSE setup failure before the first canonical stream event is fallback-
  eligible
- [ ] tests proving projector failure before the first canonical stream event is fallback-eligible
- [ ] tests proving malformed SSE framing before the first canonical stream event is fallback-eligible
- [ ] tests proving EOF or abnormal termination before the first canonical stream event is fallback-
  eligible
- [ ] tests proving finalization failure before the first canonical stream event is fallback-eligible
- [ ] tests proving projector failure after the first canonical stream event does not trigger fallback
- [ ] tests proving malformed SSE framing after the first canonical stream event does not trigger fallback
- [ ] tests proving finalization failure after the first canonical stream event does not trigger fallback
- [ ] tests proving streaming event delivery does not expose partial/live `ResponseMeta` before terminal completion
- [ ] tests proving streaming terminal success yields completed `Response` with normal `ResponseMeta`
- [ ] tests proving committed-stream terminal failure yields normalized `RuntimeError` plus `ExecutedFailureMeta` with the full ordered `AttemptRecord` history
- [ ] tests proving pre-commit streaming finalization failure yields fallback-eligible executed failure behavior when route policy allows it
- [ ] tests proving the streaming commit point is canonical-event emission to the caller, not SSE stream open and not receipt of a raw SSE frame
- [ ] adapter-planning and upstream-error normalization coverage for tools, structured output, `top_p`, stop sequences, reasoning controls, and passthrough / `extra` fields
- [ ] tests confirming that transport retries occur within one attempt and do not emit fallback behavior on their own
- [ ] non-streaming and streaming parity across the new planning boundary

## File-Sized Steps

1. Update runtime unit tests to use `TaskRequest`, `Route`, `ExecutionOptions`,
   and `ExecutionPlan`.
2. Update provider contract tests to exercise family codec and provider overlay
   composition.
3. Update transport tests to validate typed request inputs and header/timeouts
   ownership.
4. Update runtime/provider integration tests for planner, adapter, transport,
   and fallback interaction.
5. Update streaming tests for the commit-point semantics.
6. Update fixture and live tests so they stay green while proving the locked
   direct-client, routed-fallback, family-shared, and layered-native-option
   behaviors from `REFACTOR.md`.
7. Update examples so direct and routed usage both match the new public model.
8. Remove every `REFACTOR-SHIM:` marker and delete every remaining migration-
   only compatibility path that does not belong to the target architecture.

## Exit Criteria

- every checkbox above is complete
- fixture tests remain green and preserve provider payload expectations
- live tests remain green and still prove the locked behaviors called out above
- examples reflect the new public model
- no `REFACTOR-SHIM:` markers remain
- no migration-only compatibility shims from `REFACTOR.md` remain in shipped code
