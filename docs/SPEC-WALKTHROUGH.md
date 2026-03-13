# SPEC Walkthrough: docs/REFACTOR Multi-Provider Runtime Architecture

This document is a guided reading of [REFACTOR.md](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/docs/REFACTOR.md).
It is not a second source of truth. Its job is to restate the current spec in a more operational
way for people implementing or reviewing the architecture.

The current design is organized around one hard split:

- `TaskRequest`: what the model should do
- `Route`: where the call may run
- `ExecutionOptions`: how the call should execute

Everything else in the spec exists to preserve that split through planning, adapter request
construction, transport execution, streaming, fallback, and metadata reporting.

## 1. Core Mental Model

The refactor replaces the old "request plus send options plus provider overrides" model with a
route-oriented attempt model.

```rust
pub struct TaskRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
    pub response_format: ResponseFormat,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub stop: Vec<String>,
    pub metadata: BTreeMap<String, String>,
}

pub struct Route {
    pub primary: AttemptSpec,
    pub fallbacks: Vec<AttemptSpec>,
    pub fallback_policy: FallbackPolicy,
    pub planning_rejection_policy: PlanningRejectionPolicy,
}

pub struct ExecutionOptions {
    pub response_mode: ResponseMode,
    pub observer: Option<Arc<dyn RuntimeObserver>>,
    pub transport: TransportOptions,
}
```

The important boundary rules are:

- `TaskRequest` owns semantic request content only.
- `Route` owns target ordering and routing policy only.
- `ExecutionOptions` owns route-wide execution behavior only.
- provider-native controls are attempt-local, not task-wide or route-wide.
- transport execution is driven by a typed runtime-to-transport contract, not metadata tunneling.

## 2. Identity and Layering

Provider identity is split deliberately into three different concepts:

```rust
pub enum ProviderFamilyId {
    OpenAiCompatible,
    Anthropic,
}

pub enum ProviderKind {
    OpenAi,
    OpenRouter,
    Anthropic,
    GenericOpenAiCompatible,
}

pub struct ProviderInstanceId(String);
```

Use them like this:

- `ProviderFamilyId`: shared protocol-family behavior and family codecs
- `ProviderKind`: concrete adapter and overlay behavior
- `ProviderInstanceId`: one registered runtime destination with concrete config

That means two self-hosted OpenAI-compatible endpoints can share a family codec and even share
`ProviderKind::GenericOpenAiCompatible`, while still being different runtime targets because they
have different instance IDs and different `ProviderConfig` values.

This is also why routes target `ProviderInstanceId`, not `ProviderKind`.

The config split is just as important as the identity split:

- `ProviderDescriptor` is adapter-owned static metadata keyed by `ProviderKind`
- `ProviderConfig` is runtime-owned per-instance configuration keyed by `ProviderInstanceId`
- runtime composes `PlatformConfig` from `ProviderDescriptor + ProviderConfig`

That composition is where runtime resolves the effective base URL, auth style, default headers, and
default response request-id header for the selected concrete provider instance.

## 3. Public Core Types

### 3.1 Response delivery

```rust
pub enum ResponseMode {
    NonStreaming,
    Streaming,
}
```

`ResponseMode` is route-wide. It cannot vary per attempt.

The spec now locks streaming as a two-phase contract:

- canonical stream events are delivered incrementally
- one terminal completion outcome follows for the same attempt

Important consequences:

- fallback is allowed only before the first canonical stream event is emitted
- once the first canonical stream event is emitted, the streaming attempt is committed
- committed streaming failures stay on the current stream/finalization path
- streaming APIs must provide a terminal completion path such as `finalize()`, `await`, or an
  equivalent handle-level operation
- stream events themselves do not carry `ResponseMeta` or partial/live attempt-history metadata

### 3.2 Route-wide transport controls

```rust
pub struct TransportOptions {
    pub request_id_header_override: Option<String>,
    pub extra_headers: BTreeMap<String, String>,
}
```

This is the public route-wide transport control surface.

It does not own:

- HTTP method
- endpoint path selection
- response framing
- auth placement
- timeout overrides
- retry policy
- protocol-specific request hints

`request_id_header_override` affects response request-id extraction only. It does not emit or rename
an outbound header.

If no route-wide override is present, transport uses the per-instance
`PlatformConfig.request_id_header`. Runtime resolves that field from:

1. `ProviderConfig.request_id_header`, when present
2. otherwise `ProviderDescriptor.default_request_id_header`

### 3.3 Attempt-local overrides

```rust
pub struct TransportTimeoutOverrides {
    pub request_timeout: Option<Duration>,
    pub stream_setup_timeout: Option<Duration>,
    pub stream_idle_timeout: Option<Duration>,
}

pub struct AttemptExecutionOptions {
    pub native: Option<NativeOptions>,
    pub timeout_overrides: TransportTimeoutOverrides,
    pub extra_headers: BTreeMap<String, String>,
}
```

Attempt-local controls are intentionally narrow:

- layered native request options
- attempt-local timeout overrides
- attempt-local extra headers

They do not own response mode, observer override, route-wide transport settings, or protocol-level
adapter hints.

### 3.4 Native options

```rust
pub enum FamilyOptions {
    OpenAiCompatible(OpenAiCompatibleOptions),
    Anthropic(AnthropicFamilyOptions),
}

pub enum ProviderOptions {
    OpenAi(OpenAiOptions),
    Anthropic(AnthropicOptions),
    OpenRouter(OpenRouterOptions),
}

pub struct NativeOptions {
    pub family: Option<FamilyOptions>,
    pub provider: Option<ProviderOptions>,
}
```

The layering model is strict:

- family-scoped options belong to the family codec
- provider-scoped options belong to the provider overlay
- mismatched native options are planning-time incompatibilities, not silently ignored inputs

### 3.5 Targets, attempts, and routes

```rust
pub struct Target {
    pub instance: ProviderInstanceId,
    pub model: Option<String>,
}

pub struct AttemptSpec {
    pub target: Target,
    pub execution: AttemptExecutionOptions,
}

pub enum PlanningRejectionPolicy {
    FailFast,
    SkipRejectedTargets,
}
```

The effective model is resolved during planning:

1. `Target.model`, when present
2. `ProviderConfig.default_model`, when present

If neither source provides a model, planning fails before adapter planning or transport execution.
The runtime must not rely on provider-side implicit model defaults.

## 4. Attempt Metadata and Failure Surfaces

The current spec makes route attempt history first-class.

```rust
pub enum SkipReason {
    StaticIncompatibility { message: String },
    AdapterPlanningRejected { message: String },
}

pub enum AttemptDisposition {
    Skipped { reason: SkipReason },
    Succeeded { status_code: Option<u16>, request_id: Option<String> },
    Failed {
        error_kind: RuntimeErrorKind,
        error_message: String,
        status_code: Option<u16>,
        request_id: Option<String>,
    },
}

pub struct AttemptRecord {
    pub provider_instance: ProviderInstanceId,
    pub provider_kind: ProviderKind,
    pub model: String,
    pub target_index: usize,
    pub attempt_index: usize,
    pub disposition: AttemptDisposition,
}

pub struct ResponseMeta {
    pub selected_provider_instance: ProviderInstanceId,
    pub selected_provider_kind: ProviderKind,
    pub selected_model: String,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub attempts: Vec<AttemptRecord>,
}

pub struct ExecutedFailureMeta {
    pub selected_provider_instance: ProviderInstanceId,
    pub selected_provider_kind: ProviderKind,
    pub selected_model: String,
    pub status_code: Option<u16>,
    pub request_id: Option<String>,
    pub attempts: Vec<AttemptRecord>,
}

pub struct RoutePlanningFailure {
    pub reason: RoutePlanningFailureReason,
    pub attempts: Vec<AttemptRecord>,
}
```

Read the three metadata surfaces this way:

- `ResponseMeta`: success-only metadata for a completed successful call
- `ExecutedFailureMeta`: failure-side metadata for executed failures normalized into `RuntimeError`
- `RoutePlanningFailure`: failure-side metadata when routing terminates during planning before any
  executed success or executed failure

Important locked semantics:

- `AttemptRecord` is the ordered route history format used everywhere
- skipped attempts are recorded, not discarded
- `AttemptRecord.model` is always the resolved effective model for that attempt
- `ResponseMeta.selected_model` is always the concrete effective model of the successful attempt
- `ExecutedFailureMeta.selected_*` identifies the concrete executed attempt that failed
- `RoutePlanningFailure` must not invent success-only or executed-failure-only fields

## 5. Observer Semantics

Skipped attempts are not executed failures and do not emit execution lifecycle events that imply
provider execution started.

```rust
pub struct AttemptSkippedEvent {
    pub provider_instance: ProviderInstanceId,
    pub provider_kind: ProviderKind,
    pub model: String,
    pub target_index: usize,
    pub attempt_index: usize,
    pub elapsed: Duration,
    pub reason: SkipReason,
}
```

Observer rules:

- `on_attempt_start` means provider execution is about to begin
- `on_attempt_success` means an executed attempt succeeded
- `on_attempt_failure` means an executed attempt failed after execution began
- `on_attempt_skipped` means planning rejected the attempt before provider execution

For skipped attempts specifically:

- the event is emitted only after runtime resolves concrete attempt identity and effective model
- the event fields must match the corresponding `AttemptRecord`
- skipped attempts do not emit `on_attempt_start`
- skipped attempts do not emit `on_attempt_failure`

## 6. Fallback vs Planning Rejection

The walkthrough used to blur these. The refactor does not.

### Planning rejection

Planning rejections happen before any provider request is sent.

They include:

- static incompatibility such as unsupported streaming
- native-option family/provider mismatches
- requesting family/provider native options for a provider that does not support them
- deterministic local adapter-planning validation failures returned by `ProviderAdapter::plan_request`

`PlanningRejectionPolicy` governs those outcomes:

- `FailFast`: stop immediately with planning failure
- `SkipRejectedTargets`: record a skipped attempt and continue to the next target

When all remaining attempts are rejected during planning, the runtime returns
`RoutePlanningFailure`.

The two planning-time skipped-attempt reasons are:

- `SkipReason::StaticIncompatibility`
- `SkipReason::AdapterPlanningRejected`

### Executed failure

Executed failures happen after provider execution begins and are normalized into `RuntimeError`.

Fallback rules evaluate only against those normalized executed failures.

This separation is strict:

- planning rejections do not enter fallback evaluation
- fallback does not inspect raw transport responses
- committed streaming failures are executed failures, but they are not fallback-eligible once the
  stream is committed

## 7. FallbackPolicy

`FallbackPolicy` now answers only one question: should routing advance to the next attempt after an
executed failure?

```rust
pub enum FallbackAction {
    RetryNextTarget,
    Stop,
}

pub struct FallbackMatch {
    pub error_kinds: Vec<RuntimeErrorKind>,
    pub status_codes: Vec<u16>,
    pub provider_codes: Vec<String>,
    pub provider_kinds: Vec<ProviderKind>,
    pub provider_instances: Vec<ProviderInstanceId>,
}

pub struct FallbackRule {
    pub when: FallbackMatch,
    pub action: FallbackAction,
}

pub struct FallbackPolicy {
    pub rules: Vec<FallbackRule>,
}
```

Locked behavior:

- targets live on `Route`, not on `FallbackPolicy`
- rules are evaluated in insertion order
- first match wins
- `FallbackMatch` uses AND semantics across non-empty fields
- provider-code matching is meaningful only after family/provider error decode and runtime
  normalization populated a provider code
- `RetryNextTarget` advances to the next attempt already present on the route
- `Stop`, or no matching rule, surfaces the current error

## 8. Runtime, Adapter, Codec, Overlay, and Transport

### Runtime owns

- input normalization into `TaskRequest`
- route iteration
- model resolution
- `ProviderInstanceId` resolution into `RegisteredProvider`
- adapter selection by `ProviderKind`
- `PlatformConfig` composition from `ProviderDescriptor` and `ProviderConfig`
- capability validation and planning-rejection handling
- `ExecutionPlan` construction
- normalization of route-wide and attempt-local transport inputs
- runtime classification of streaming commit and fallback eligibility
- error normalization and fallback orchestration

### Family codec owns

- shared family request encoding
- family-scoped native options
- family response decode
- family error decode
- default family stream projector

### Provider overlay owns

- provider-specific request augmentation
- provider-scoped native options
- provider-specific error refinement
- optional success-decode overrides
- optional stream-projector overrides
- provider-specific endpoint override when needed
- provider-generated dynamic headers

### Provider adapter owns

- orchestration of codec plus overlay into `ProviderRequestPlan`
- deterministic request-shape validation during planning
- success decode entry point
- stream-projector selection entry point

The orchestration rules are:

- on non-streaming success, the provider overlay gets first chance to override decode
- if the overlay does not override, the family codec success decode runs
- on streaming success, the provider overlay gets first chance to override stream-projector
  creation
- if the overlay does not override, the family codec default projector is used

### Transport owns

- outbound HTTP/SSE execution
- auth placement
- header materialization
- retries within a single attempt
- request and stream timeout enforcement
- JSON/bytes/SSE framing mechanics
- SSE parsing and limit enforcement without provider semantics

The transport does not decide fallback, decode provider-specific error bodies, or invent methods,
URLs, or response framing.

## 9. Typed Runtime-to-Transport Boundary

The runtime-to-transport handoff is typed and explicit.

```rust
pub enum TransportResponseFraming {
    Json,
    Bytes,
    Sse,
}

pub struct TransportExecutionInput {
    pub platform: PlatformConfig,
    pub auth_token: Option<AuthCredentials>,
    pub method: Method,
    pub url: String,
    pub body: HttpRequestBody,
    pub response_framing: TransportResponseFraming,
    pub request_options: HttpRequestOptions,
    pub transport: ResolvedTransportOptions,
    pub provider_headers: HeaderMap,
}
```

This boundary is one of the most important changes in the spec.

Key rules:

- the final URL is runtime-resolved before transport execution
- `ProviderRequestPlan.method` is the single source of truth for outbound HTTP method
- `ProviderRequestPlan.response_framing` is the single source of truth for transport response
  framing
- runtime validates adapter-produced framing against `ExecutionOptions.response_mode`
- `ResponseMode::NonStreaming` must not produce `TransportResponseFraming::Sse`
- `ResponseMode::Streaming` must produce `TransportResponseFraming::Sse`
- transport receives caller-owned transport headers separately from adapter-produced provider headers
- metadata-based transport override conventions are retired
- `AdapterContext` is not part of the long-term transport contract

### Header layering

Transport materializes headers in this exact order:

1. platform default headers
2. route-wide extra headers
3. attempt-local extra headers
4. adapter/provider-generated dynamic headers
5. auth headers

`request_id_header_override` is not part of that merge order. It only changes response request-id
extraction precedence.

Request-id extraction precedence is:

1. `ResolvedTransportOptions.request_id_header_override`, when present
2. otherwise `PlatformConfig.request_id_header`

### Timeouts and retries

Typed runtime-facing timeout control includes:

- request timeout
- stream-setup timeout
- stream-idle timeout

There is no separate public first-byte timeout field. First-byte behavior remains transport-internal
and is governed by resolved `stream_idle_timeout`.

Transport retry remains intra-attempt behavior. It is distinct from routed fallback.

## 10. Planning and Execution Flow

### 10.1 Direct client normalization

Direct concrete clients still preserve ergonomic usage:

```rust
openai_client.messages().create(input).await?;
openai_client.messages().model("gpt-5").create(input).await?;
openai_client.streaming().create(input).await?;
openrouter_client.messages().create_with_options(input, native_options).await?;
```

Internally, a direct client call normalizes into:

1. `TaskRequest`
2. a single-attempt `Route` targeting the client's configured `ProviderInstanceId`
3. inferred `ExecutionOptions`

Direct fluent `.model(...)` configuration does not mutate `TaskRequest`. It becomes
`AttemptSpec.target.model` on the generated single-attempt route.

### 10.2 Routed toolkit usage

High-level routed APIs remain the main multi-provider entry point:

```rust
toolkit.messages().create(input, route).await?;
toolkit.messages().create_with_meta(input, route).await?;
toolkit.messages().create_with_execution(input, route, execution_options).await?;
toolkit.streaming().create(input, route).await?;
toolkit.streaming().create_with_execution(input, route, execution_options).await?;
```

Semantics:

- `.messages()` implies `ResponseMode::NonStreaming`
- `.streaming()` implies `ResponseMode::Streaming`
- ergonomic routed methods infer default `ExecutionOptions`
- explicit routed methods accept per-call `ExecutionOptions`

### 10.3 Routed execution flow

The routed flow is:

1. normalize input into `TaskRequest`
2. iterate ordered `AttemptSpec`s from `Route`
3. for each attempt:
   - resolve target instance into `RegisteredProvider`
   - resolve effective model
   - resolve adapter by `ProviderKind`
   - resolve provider config from the registered instance
   - compose `PlatformConfig` from `ProviderDescriptor + ProviderConfig`
   - validate static capabilities
   - apply `PlanningRejectionPolicy`
   - if planning rejects and skipping is enabled, record a skipped `AttemptRecord`, emit
     `on_attempt_skipped`, and continue
   - otherwise build `ResolvedProviderAttempt`
   - build `ExecutionPlan`
   - ask the adapter to produce `ProviderRequestPlan`
   - build `TransportExecutionInput`
   - execute transport request
4. after an executed failure, evaluate `FallbackPolicy` against normalized `RuntimeError`
5. on success, return `ResponseMeta`
6. on executed failure, return normalized `RuntimeError` plus `ExecutedFailureMeta`

## 11. Streaming Commit and Finalization

The current spec is explicit about the streaming cutoff.

Pre-commit streaming failures are fallback-eligible executed failures, including:

- SSE setup failure before any canonical stream event
- malformed SSE framing before any canonical stream event
- projector failure before any canonical stream event
- abnormal termination before any canonical stream event
- stream finalization failure before any canonical stream event

Post-commit streaming failures are not fallback-eligible, including:

- malformed SSE framing after at least one canonical event
- projector failure after at least one canonical event
- abnormal termination after at least one canonical event
- stream finalization failure after at least one canonical event

The caller-visible consequence is:

- stream events are incremental delivery only
- terminal success yields a completed canonical `Response` with normal `ResponseMeta`
- terminal executed failure yields normalized `RuntimeError` plus `ExecutedFailureMeta`

## 12. Error Decoding and Normalization

When execution fails after transport begins, the runtime owns normalization.

The locked non-success path is:

1. transport returns a non-success response or transport/status failure
2. if the adapter enabled `preserve_error_body_for_adapter_decode`, runtime preserves the body
3. runtime invokes family-codec error decode
4. runtime invokes provider-overlay error decode
5. runtime merges results with provider-overlay fields taking precedence on collision
6. runtime normalizes the merged result into `RuntimeError`
7. `FallbackPolicy` evaluates the normalized executed failure

The successful path follows the same adapter-orchestrated layering:

1. in non-streaming mode, adapter success decode checks provider-overlay override first
2. if the overlay does not override, family-codec success decode runs
3. in streaming mode, adapter stream-projector selection checks provider-overlay override first
4. if the overlay does not override, the family-codec projector is used
5. runtime, not the adapter or projector, still owns streaming commit classification and fallback
   eligibility

Important consequences:

- fallback never runs against raw HTTP bodies
- provider-code matching happens only after decode plus normalization
- family decode runs before provider overlay decode
- transport does not interpret provider-specific error payloads for fallback purposes

## 13. Public API Examples

### 13.1 Routed non-streaming

```rust
let route = Route::to(
    AttemptSpec::to(
        Target::new(ProviderInstanceId::new("openai-default")).with_model("gpt-5")
    )
)
.with_fallback(
    AttemptSpec::to(
        Target::new(ProviderInstanceId::new("openrouter-default"))
            .with_model("openai/gpt-5")
    )
    .with_native_options(
        NativeOptions {
            family: Some(FamilyOptions::OpenAiCompatible(
                OpenAiCompatibleOptions {
                    parallel_tool_calls: Some(true),
                    ..Default::default()
                }
            )),
            provider: Some(ProviderOptions::OpenRouter(
                OpenRouterOptions::new().with_route("fallback")
            )),
        }
    )
)
.with_policy(FallbackPolicy::default())
.with_planning_rejection_policy(PlanningRejectionPolicy::FailFast);

let response = toolkit.messages().create(input, route).await?;
```

### 13.2 Routed with explicit execution overrides

```rust
let execution = ExecutionOptions {
    response_mode: ResponseMode::NonStreaming,
    observer: None,
    transport: TransportOptions {
        request_id_header_override: Some("x-request-id".into()),
        extra_headers: BTreeMap::from([("x-trace-id".into(), "abc123".into())]),
    },
};

let (response, meta) = toolkit
    .messages()
    .create_with_meta_and_execution(input, route, execution)
    .await?;
```

### 13.3 Direct client with native options

```rust
let response = openai_client
    .messages()
    .create_with_options(
        input,
        NativeOptions {
            family: Some(FamilyOptions::OpenAiCompatible(
                OpenAiCompatibleOptions {
                    reasoning: Some(OpenAiReasoning::default()),
                    ..Default::default()
                }
            )),
            provider: Some(ProviderOptions::OpenAi(
                OpenAiOptions {
                    service_tier: Some("priority".into()),
                    ..Default::default()
                }
            )),
        },
    )
    .await?;
```

## 14. Practical Reading Guide

If you are consuming the API, keep these rules in your head:

- build semantic task content once on `TaskRequest` or `MessageCreateInput`
- build destination choices explicitly as `AttemptSpec`s inside a `Route`
- keep route-wide execution behavior on `ExecutionOptions`
- keep target-specific native controls, timeout overrides, and extra headers on
  `AttemptExecutionOptions`
- expect ordered attempt history on success, executed failure, and planning failure surfaces

If you are implementing the runtime, keep these invariants in your head:

- resolve ambiguity before adapter planning
- adapters plan provider intent, not route behavior
- runtime owns fallback and streaming commit classification
- transport executes typed intent, not provider semantics

In one line, the current architecture is:

`TaskRequest + Route + ExecutionOptions -> ExecutionPlan -> ProviderRequestPlan -> TransportExecutionInput -> canonical response/stream or normalized failure with attempt metadata`.
