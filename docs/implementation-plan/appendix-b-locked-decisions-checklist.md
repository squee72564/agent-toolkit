# Appendix B: Locked Decisions Checklist

<!-- Verification completed: 2026-03-13 -->

Every item below comes from the "Final Design Decisions Locked In" section of
[docs/REFACTOR.md](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/docs/REFACTOR.md).
Do not declare the refactor done until every item is checked.

## Core Request and Execution Model

- [x] `Request` is replaced conceptually by `TaskRequest + Route + ExecutionOptions`
- [x] model lives on the attempt target path, not the task
- [x] stream is replaced by `ExecutionOptions.response_mode`
- [x] `SendOptions` is retired and replaced by `Route` plus `ExecutionOptions`
- [x] observer injection lives on `ExecutionOptions`
- [x] routed ergonomic methods may infer default `ExecutionOptions`, but explicit routed APIs accept per-call execution options

## Typed Transport Ownership

- [x] transport options are typed and explicit, not a generic metadata map
- [x] `agent-transport` remains the transport boundary and receives typed runtime-facing inputs
- [x] `AdapterContext` is retired from the long-term transport request contract
- [x] metadata-based transport override conventions are removed from the target architecture
- [x] typed timeout fields are runtime-owned end-to-end and are never clamped or overridden by adapters
- [x] overlap between route-wide transport options, attempt-local execution overrides, and adapter protocol hints is intentionally minimized
- [x] route-wide transport options own only call-wide transport concerns
- [x] attempt-local execution overrides own only attempt-local transport concerns, including typed request, stream-setup, and stream-idle timeout overrides
- [x] provider adapters own protocol-specific request planning artifacts, including `HttpRequestOptions`, outbound HTTP method, and transport response framing
- [x] adapter-controlled preservation of non-success response bodies for provider error decoding is a protocol-specific `HttpRequestOptions` concern
- [x] provider adapters may emit provider-generated dynamic headers through request planning and those headers are carried separately from `ResolvedTransportOptions`
- [x] provider adapters are the only layer that selects outbound HTTP method and transport response framing
- [x] runtime owns normalization of typed transport inputs before calling transport
- [x] runtime resolves the effective endpoint path using `ProviderRequestPlan.endpoint_path_override` when present and `ProviderDescriptor.endpoint_path` otherwise
- [x] runtime constructs the final outbound URL before calling transport
- [x] runtime validates `ProviderRequestPlan.response_framing` against `ExecutionOptions.response_mode` before calling transport
- [x] transport owns final header materialization and merges platform defaults, route-wide headers, attempt-local headers, adapter/provider headers, and auth in that order
- [x] transport receives a fully resolved URL and does not invent endpoint paths
- [x] transport receives explicit method and response framing and does not infer either one
- [x] `request_id_header_override` overrides response request-id extraction only and does not materialize an outbound request header
- [x] provider-kind default request-id header selection lives on `ProviderDescriptor`
- [x] provider-instance request-id header override lives on `ProviderConfig`
- [x] `PlatformConfig.request_id_header` is the effective per-instance default used when `request_id_header_override` is absent

## Fallback and Error Handling

- [x] fallback is evaluated only after runtime normalizes an executed failure into `RuntimeError`
- [x] `FallbackPolicy` is rule-driven in the target architecture
- [x] `FallbackPolicy.rules` are evaluated in insertion order and first match wins
- [x] `FallbackMatch` uses AND semantics across all non-empty fields
- [x] `FallbackAction::RetryNextTarget` advances to the next attempt already present on `Route`
- [x] `FallbackAction::Stop`, or no matching rule, stops routing and surfaces the current error
- [x] fallback rules may match by `ProviderKind` and/or `ProviderInstanceId`
- [x] legacy fallback toggles such as `retry_on_status_codes`, `retry_on_transport_error`, and `FallbackMode` are migration-only compatibility behavior, not target architecture
- [x] family-codec error decoding runs before provider-overlay error decoding when the adapter requested error-body preservation
- [x] provider-overlay error decoding may refine or override family-decoded fields before runtime normalization
- [x] transport does not interpret provider-specific error bodies for fallback purposes

## Identity and Provider Composition

- [x] `ProviderDescriptor` is adapter-owned static metadata
- [x] `ProviderKind` identifies concrete adapter and overlay behavior
- [x] `ProviderInstanceId` identifies one registered runtime destination
- [x] routes target `ProviderInstanceId`, not `ProviderKind`
- [x] runtime resolves `ProviderInstanceId` into a registered provider carrying `ProviderKind` and `ProviderConfig`, then resolves `ProviderFamilyId` from the selected provider descriptor
- [x] `ProviderConfig` is runtime-owned provider-instance configuration
- [x] `PlatformConfig` is the resolved transport-facing result of `ProviderDescriptor + ProviderConfig`
- [x] runtime, not adapters, composes `PlatformConfig` in the target architecture
- [x] `AttemptSpec` is a routing-layer type only and is consumed before adapter planning
- [x] `ExecutionPlan` carries `ResolvedProviderAttempt`, not `AttemptSpec`

## Retry, Streaming, and Capability Rules

- [x] first-byte timeout remains transport-internal and is governed by stream-idle timeout behavior
- [x] transport retry and routed fallback remain separate mechanisms with separate ownership
- [x] non-streaming mode and streaming mode are strictly separate public execution contracts
- [x] streaming mode may finalize into a completed canonical response after stream completion
- [x] non-streaming mode does not internally open SSE streams and finalize them
- [x] non-streaming execution is the baseline provider contract and streaming is the optional additional capability checked during static capability validation
- [x] static capability mismatch is intentionally narrow and limited to locked high-confidence invariants
- [x] adapter-planning rejection is a distinct pre-execution planning outcome, not an executed failure
- [x] model-level or deployment-level feature support is not inferred from provider-level static capabilities in the baseline architecture
- [x] fine-grained request features such as tools, structured output, `top_p`, stop sequences, reasoning controls, and passthrough / `extra` fields are validated during adapter planning and/or the upstream response path and then surfaced as planning-time rejection or normalized executed failure

## Routing and Native Options

- [x] fallback targets live on `Route`, not `FallbackPolicy`
- [x] layered native options are target-scoped through `AttemptSpec`
- [x] native options are layered into family-scoped and provider-scoped parts through `NativeOptions`
- [x] family-scoped native options must match the attempt target family
- [x] provider-scoped native options must match the resolved provider kind
- [x] non-matching native options are static capability mismatches, not ignored inputs
- [x] family codecs consume family-scoped native options
- [x] provider overlays consume provider-scoped native options
- [x] `ProviderRequestPlan.endpoint_path_override` is the only adapter-controlled endpoint-selection mechanism
- [x] attempt-local execution overrides are allowed, but only for attempt-local behavior

## Public API Ergonomics and Registration Model

- [x] direct concrete clients preserve `.model(...)`-style per-call ergonomics by normalizing model selection into their generated single-attempt route targeting their configured registered instance
- [x] multiple registered instances may share the same `ProviderKind` and `ProviderFamilyId`
- [x] `GenericOpenAiCompatible` is a first-class provider kind for self-hosted OpenAI-compatible endpoints

## Planning Rejection and Attempt Metadata

### Phase 03: Planning-Only Metadata (Complete)

- [x] static capability mismatch fails fast by default
- [x] optional skip-on-planning-rejection is supported through `PlanningRejectionPolicy`
- [x] skipped attempts are first-class `AttemptRecord` outcomes, not failed executions
- [x] `SkipReason::StaticIncompatibility` and `SkipReason::AdapterPlanningRejected` are distinct skipped-attempt reasons
- [x] `AttemptRecord` includes `provider_instance`, `provider_kind`, concrete resolved `model`, `target_index`, and `attempt_index`
- [x] skipped attempts never carry provider request id or executed-attempt status metadata
- [x] `RoutePlanningFailure` is the planning-only failure surface and never carries success-only or executed-failure-only selected-attempt metadata
- [x] `RoutePlanningFailureReason` preserves the `NoCompatibleAttempts` vs `AllAttemptsRejectedDuringPlanning` distinction
- [x] runtime does not create an `AttemptRecord` or skipped-attempt observer event when effective model resolution fails before a concrete attempt is resolved

### Phase 09: Execution Metadata Unification (Deferred)

- [x] skipped attempts emit `on_attempt_skipped`, not `on_attempt_start` or `on_attempt_failure`
- [x] executed-failed `AttemptRecord` values carry normalized failure kind/message and execution-derived status/request-id fields only when available
- [x] returned route metadata is ordered attempt history, including both skipped and executed attempts, and records both provider instance and provider kind
- [x] `ResponseMeta` is success-only metadata and records the successful attempt's selected provider instance, selected provider kind, concrete resolved selected model, and final status/request-id fields when available
- [x] `ResponseMeta.attempts` uses `AttemptRecord` shape (not `Vec<AttemptMeta>`)
- [x] `ExecutedFailureMeta` is the executed-failure metadata surface, records the failed attempt's selected provider instance, selected provider kind, concrete resolved selected model, and final status/request-id fields when available, and is never used for planning-only outcomes
- [x] `ResponseMeta.attempts`, `ExecutedFailureMeta.attempts`, and `RoutePlanningFailure.attempts` use the same `AttemptRecord` shape, ordering rules, and skip semantics
- [x] `on_attempt_skipped` carries the resolved skipped-attempt identity/order fields plus elapsed planning time
- [x] `on_attempt_start`, `on_attempt_success`, and `on_attempt_failure` are execution-only lifecycle callbacks; skipped attempts emit only `on_attempt_skipped`
- [x] incremental stream events never expose partial/live `ResponseMeta` or route-attempt metadata; terminal streaming success yields normal `ResponseMeta`, and terminal streaming executed failure yields normalized `RuntimeError` plus `ExecutedFailureMeta`

## Provider Families and Overlays

- [x] provider families are modeled explicitly via codecs and provider-specific differences are modeled via overlays
