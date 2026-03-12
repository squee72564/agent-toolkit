# REFACTOR2.md: Multi-Provider Runtime Architecture

  ## Preface

  ### Problem

  The current runtime architecture conflates several concerns that need to be modeled separately:

  - the semantic task being asked of the model
  - the concrete provider/model destination used to execute that task
  - layered native request controls, especially for providers that share a protocol family but
    expose extra concrete-provider fields
  - execution behavior such as streaming, observer overrides, and transport options
  - routing and fallback policy across multiple candidate targets

  This creates tension in several places:

  - Request mixes task semantics with execution concerns like model_id and stream
  - SendOptions mixes routing, transport metadata, and observer overrides
  - ProviderAdapter::plan_request(req) is not target-aware
  - routing reuses one request across fallback targets even when native controls should differ per
    target
  - provider-family reuse exists informally, but family-scoped native controls and provider-specific
    overlays are not first-class
  - provider identity is overloaded across protocol family, concrete adapter behavior, and concrete
    registered destination instance

  ### Goal

  Redesign the architecture so that it is:

  - explicit about boundaries and ownership
  - typed end-to-end, with no stringly-typed provider override channel
  - cleanly extensible to new providers and new provider families
  - ergonomic for direct single-provider clients
  - expressive for multi-provider routing and fallback
  - compatible with the existing mental model of direct clients, routed toolkit execution, and
    canonical streaming
  - robust in the presence of provider-family shared behavior plus layered native request
    differences

  ### Design Intent

  The new design should preserve the existing core abstractions:

  - direct concrete clients like OpenAiClient, OpenRouterClient, AnthropicClient
  - routed execution through AgentToolkit
  - high-level task builders such as MessageCreateInput
  - canonical response and stream surfaces
  - transport isolation in crates/agent-transport

  But it should enforce a cleaner internal model:

  - TaskRequest describes what to do
  - Route describes where it can run
  - ExecutionOptions describes how the call should be executed
  - ExecutionPlan describes one fully resolved attempt
  - ProviderFamilyId describes shared protocol-family behavior
  - ProviderKind describes concrete adapter and overlay behavior
  - ProviderInstanceId describes one registered runtime destination
  - provider-family codecs handle shared wire behavior
  - provider overlays handle provider-specific behavior

  ## Current Design Bottlenecks

  - Request currently mixes semantic task fields with execution concerns like model_id and stream.
  - SendOptions currently mixes route selection, fallback policy, transport metadata, and observer
    injection.
  - FallbackPolicy currently owns fallback targets as well as fallback decision rules.
  - provider-specific request extras do not have a stable, typed public model
  - ProviderAdapter::plan_request(req) cannot inspect target-local native options because the target
    is not part of the planning contract
  - OpenAI-compatible providers reuse behavior, but that reuse is not formalized as a family codec
    plus provider overlay
  - transport control is effectively tunneled through metadata rather than represented as a typed
    contract across the agent-runtime -> agent-transport boundary
  - AdapterContext is part of the current transport request path even though transport overrides
    should be typed and explicit
  - HttpSendRequest currently depends on AdapterContext for header and request-id override behavior
  - current timeout and header override behavior is split implicitly across transport defaults,
    request options, and metadata conventions
  - direct-client and routed-client flows share intent, but are not normalized into one coherent
    attempt model
  - `ProviderId` is currently forced to stand in for both adapter identity and runtime destination
    identity

  ## Core Architectural Rules

  ### Rule 1

  TaskRequest owns semantic request content only.

  It must not own:

  - provider instance selection
  - model selection
  - fallback behavior
  - streaming mode
  - transport options
  - observer overrides
  - layered native request controls

  ### Rule 2

  Route owns target selection and fallback topology only.

  It must own:

  - primary attempt target
  - ordered fallback attempt targets
  - fallback decision policy
  - capability mismatch handling policy

  It must not own:

  - semantic task content
  - response mode
  - observer overrides
  - transport-wide execution settings

  ### Rule 3

  ExecutionOptions owns per-call execution behavior that is not routing.

  It owns:

  - response delivery mode (non-streaming vs streaming)
  - observer override
  - typed transport options

  It must not own:

  - task semantics
  - fallback target ordering
  - family-scoped or provider-scoped native request-body controls

  ### Rule 4

  ExecutionPlan is the single resolved-attempt contract.

  It is the handoff between:

  - route resolution
  - task validation
  - adapter request planning
  - transport execution

  ### Rule 5

  Layered native request controls are target-scoped, not task-scoped.

  If a routed request may attempt multiple providers, each attempt carries only the native options
  for that target family/provider pair.

  Family-scoped native options must match the selected attempt target family.
  Provider-scoped native options must match the selected resolved provider kind.

  A mismatched native option is a static incompatibility, not an ignored field.

  ### Rule 6

  Provider-family shared behavior is implemented through family codecs; provider-specific differences
  are implemented through overlays.

  ## Primary Layers and Responsibilities

  ### 1. Task Layer

  Owns the semantic task.

  Responsibilities:

  - messages
  - tools
  - tool choice
  - response format
  - shared generation controls
  - semantic request metadata intended for provider payloads when supported

  ### 2. Routing Layer

  Owns where a task may run.

  Responsibilities:

  - primary target
  - fallback targets
  - fallback rules
  - capability mismatch policy
  - target-local layered native options
  - target-local attempt overrides

  ### 3. Execution Layer

  Owns how a logical call is executed.

  Responsibilities:

  - non-streaming vs streaming
  - observer injection
  - typed transport options
  - per-call execution context

  ### 4. Planning Layer

  Owns conversion of task + selected attempt into a concrete provider attempt.

  Responsibilities:

  - model resolution
  - capability validation
  - registered-provider resolution
  - provider config selection from the resolved provider instance
  - normalized resolved-attempt creation
  - execution-plan construction

  ### 5. Provider Layer

  Owns provider wire behavior.

  Responsibilities:

  - family codec request encoding
  - provider overlay request augmentation
  - provider-specific response and error decoding
  - provider-specific streaming quirks
  - capability exposure

  ### 6. Transport Layer

  Owns HTTP/SSE mechanics and request execution against provider endpoints.

  Responsibilities:

  - construct outbound HTTP requests from:
    - provider platform configuration
    - resolved auth credentials
    - typed route-wide transport options
    - typed attempt-local transport overrides
    - adapter-produced provider headers
    - adapter-produced request body
    - adapter-produced protocol-level request options
  - place auth headers according to platform auth style
  - apply transport retry policy before response body handoff
  - enforce request, stream-setup, and stream-idle timeouts
  - execute JSON, bytes, and SSE response modes
  - produce low-level response framing via HttpResponseHead, HttpJsonResponse, HttpBytesResponse, and
    HttpSseResponse
  - parse and enforce SSE framing and limits without provider-specific semantics

  Runtime provides:

  - resolved provider/platform config
  - resolved auth credentials
  - adapter-produced provider headers
  - adapter-produced request body
  - adapter-produced HttpRequestOptions
  - typed route-wide and attempt-local transport inputs normalized into the transport request contract
  - expected response mode

  Transport returns:

  - HttpJsonResponse
  - HttpBytesResponse
  - or HttpSseResponse

  Locked transport-boundary decisions:

  - agent-transport is a first-class typed boundary
  - AdapterContext is not part of the long-term transport request contract
  - transport metadata magic keys such as transport.request_id_header and transport.header.* are
    removed from the architecture
  - HttpSendRequest, or a direct typed successor to it, is the runtime-to-transport request contract
  - runtime is responsible for normalizing route-wide and attempt-local typed transport inputs before
    calling agent-transport
  - transport is responsible for materializing final headers, applying auth, enforcing timeouts,
    applying pre-body retries, and framing responses
  - request, stream-setup, and stream-idle timeout fields are explicit typed transport inputs
  - first-byte timeout remains internal to transport and is governed by stream-idle timeout behavior

  Recommended Type Direction:

  The redesigned runtime should eventually normalize into a transport-facing shape conceptually like:

  ```rust
  pub struct TransportExecutionInput {
      pub platform: PlatformConfig,
      pub auth_token: Option<AuthCredentials>,
      pub method: Method,
      pub url: String,
      pub body: HttpRequestBody,
      pub response_mode: HttpResponseMode,
      pub request_options: HttpRequestOptions,
      pub transport: ResolvedTransportOptions,
      pub provider_headers: HeaderMap,
  }
  ```

Where `ResolvedTransportOptions` is derived from:

- route-wide ExecutionOptions.transport fields
- attempt-local header overrides
- attempt-local timeout overrides
- provider/platform defaults

  The typed timeout fields carried through this boundary are:

  - request timeout
  - stream-setup timeout
  - stream-idle timeout

  Provider adapters do not modify typed timeout fields.

Typed timeout selection is runtime-owned end-to-end:

- caller/runtime chooses attempt-local timeout override values
- runtime resolves them against provider/runtime defaults
- transport enforces the resolved values

  Adapter-produced protocol hints do not include timeout mutation.

  Final URL resolution is runtime-owned end-to-end:

  - `ProviderDescriptor.endpoint_path` is the default endpoint path
  - `ProviderRequestPlan.endpoint_path_override` replaces that default path when present
  - runtime resolves the effective endpoint path before transport execution
  - runtime joins `PlatformConfig.base_url` with the effective endpoint path to produce
    `TransportExecutionInput.url`
  - transport receives a fully resolved `url` and does not invent endpoint paths

  This contract may extend the existing HttpSendRequest or replace it with a directly equivalent
  type, but the architecture is locked to this typed boundary rather than metadata-driven transport
  control.

  Proposed implementation direction:

  - extend the current `HttpSendRequest` shape, or its direct successor, with an explicit
    `provider_headers` field
  - update `agent-transport` header construction to consume explicit header layers instead of
    `AdapterContext.metadata`

  ## Runtime / Provider / Transport Boundary

  ### Boundary Rule

  The runtime/provider/transport boundary is locked as follows:

  - runtime owns route resolution, target selection, capability validation, and transport-input
    normalization
  - provider adapters own provider request-body planning and protocol-specific request hints
  - transport owns outbound request construction, auth placement, retry/timeouts, and low-level
    response framing

  ### Long-Term Transport Contract

  The long-term transport boundary must not depend on AdapterContext.metadata.

  The architecture will replace metadata-based transport control with a typed request contract
  conceptually equivalent to:

  ```rust
  pub struct TransportExecutionInput {
      pub platform: PlatformConfig,
      pub auth_token: Option<AuthCredentials>,
      pub method: Method,
      pub url: String,
      pub body: HttpRequestBody,
      pub response_mode: HttpResponseMode,
      pub request_options: HttpRequestOptions,
      pub transport: ResolvedTransportOptions,
      pub provider_headers: HeaderMap,
  }
  ```

  This contract may extend the existing HttpSendRequest or replace it with a directly equivalent
  type, but the spec locks in the behavior and ownership, not a temporary compatibility shape.

  Locked URL-construction rule:

  - `TransportExecutionInput.url` is fully resolved by runtime before transport execution
  - transport does not combine `PlatformConfig.base_url` with endpoint paths
  - adapter-controlled endpoint selection happens only through `ProviderRequestPlan.endpoint_path_override`

  ### AdapterContext Decision

  AdapterContext is retired from the long-term transport boundary.

  Migration shims may temporarily synthesize AdapterContext internally while runtime and transport
  are being refactored, but the target architecture is:

  - no transport control through AdapterContext.metadata
  - no public dependence on metadata conventions for header injection or request-id extraction
  - auth credentials and transport overrides passed explicitly as typed fields

  ## Canonical Core Types

  ## 1. TaskRequest

  ### Purpose

  Represents only the semantic request being sent to a model.

  ### Public Shape

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
  ```

  ### Notes

  - model_id is removed
  - stream is removed
  - layered native options are removed
  - this replaces the semantic role of the current Request

  ## 2. ResponseMode

  ### Purpose

  Represents how a call delivers results.

  ### Shape

  ```rust
  pub enum ResponseMode {
      NonStreaming,
      Streaming,
  }
  ```

  ### Rule

  ResponseMode is route-wide. It cannot vary per attempt.

  Locked semantics:

  - `ResponseMode::NonStreaming` means the caller receives one completed canonical `Response`
  - `ResponseMode::NonStreaming` is the baseline provider execution contract for providers supported
    by this runtime
  - `ResponseMode::NonStreaming` requires the provider's normal non-streaming HTTP request/response
    path, such as `stream: false` or omission of a streaming flag
  - `ResponseMode::NonStreaming` does not open SSE streams and does not internally drain/finalize a
    streaming attempt
  - `ResponseMode::Streaming` means the caller receives canonical stream events from an opened SSE
    attempt requested in streaming mode
  - `ResponseMode::Streaming` is an optional provider capability
  - response mode is resolved before adapter request planning and is an input to planning, not an
    inference from the returned request plan
  - adapters must produce a request plan consistent with the requested response mode
  - returning an SSE transport plan for `ResponseMode::NonStreaming` is a planning error
  - streaming APIs may expose a finalize/await step that yields the completed canonical `Response`
    after the stream finishes
  - stream finalization is part of the streaming API contract, not a non-streaming transport path

  ## 3. TransportOptions

  ### Purpose

  Represents route-wide transport execution controls that apply to the logical call as a whole.

  ### Public Shape

  ```rust
  pub struct TransportOptions {
      pub request_id_header_override: Option<String>,
      pub extra_headers: BTreeMap<String, String>,
  }
  ```

  ### Notes

  - this is the public runtime-facing transport control surface
  - it is normalized into whatever typed request structures agent-transport needs internally
  - no generic transport metadata map remains as the primary API
  - this type only owns transport concerns that are truly call-wide
  - provider/protocol request hints do not live here
  - `request_id_header_override` changes response request-id extraction only
  - `request_id_header_override` does not materialize or modify any outbound request header
  - if a caller wants to send a request header with that same name, it must be supplied through
    normal extra-header fields instead

  ## 4. ResolvedTransportOptions

  ### Purpose

  Represents the transport-owned, fully normalized result of combining transport defaults with
  route-wide and attempt-local transport settings before the transport request is issued.

  ### Internal Shape

  ```rust
  pub struct TransportTimeoutOverrides {
      pub request_timeout: Option<Duration>,
      pub stream_setup_timeout: Option<Duration>,
      pub stream_idle_timeout: Option<Duration>,
  }

  pub struct ResolvedTransportOptions {
      pub request_id_header_override: Option<String>,
      pub extra_headers: BTreeMap<String, String>,
      pub timeouts: TransportTimeoutOverrides,
  }
  ```

  ### Notes

  - produced by runtime during execution-plan resolution
  - consumed when constructing the transport request contract
  - contains only runtime-owned transport fields
  - does not include protocol-specific request hints from adapters; those stay in
    `HttpRequestOptions`
  - does not include adapter-produced provider headers; those are carried separately in the
    transport execution input
  - `request_id_header_override` overrides the response-header lookup used to extract request ids
    from response metadata; it does not participate in outbound header construction
  - first-byte timeout remains an internal transport concern and is governed by `stream_idle_timeout`

  ## 5. ExecutionOptions

  ### Purpose

  Represents per-call execution behavior that is not task semantics and not routing.

  ### Public Shape

  ```rust
  pub struct ExecutionOptions {
      pub response_mode: ResponseMode,
      pub observer: Option<Arc<dyn RuntimeObserver>>,
      pub transport: TransportOptions,
  }
  ```

  ### Notes

  - replaces the execution-oriented responsibilities of SendOptions
  - direct-client high-level methods infer this automatically
  - routed high-level methods infer response_mode from .messages() vs .streaming()

  ## 6. NativeOptions

  ### Purpose

  Represents optional target-scoped native request controls layered by protocol family and concrete
  provider.

  This is the locked public model for native request controls in the redesigned runtime.

  ### Public Shape

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

  ### Layering Model

  `NativeOptions` is intentionally layered:

  - `family` carries controls shared by every provider in a protocol family
  - `provider` carries controls specific to one concrete provider within that family

  The family codec consumes `family`.
  The provider overlay consumes `provider`.

  ### Semantics

  - `NativeOptions` is attempt-local and target-scoped
  - `family` is optional
  - `provider` is optional
  - both may be present on the same attempt when the target requires shared family controls plus
    provider-specific controls
  - native options never propagate across providers or across attempts in routing
  - native options do not live on `TaskRequest`
  - a mismatched family option or provider option is a static capability mismatch, not an ignored
    field

  ### Validation Rule

  During attempt planning:

  1. read `AttemptSpec.target.instance`
  2. resolve the registered provider instance
  3. resolve the provider kind from that registered instance
  4. resolve the target family from the selected provider descriptor
  5. if `AttemptExecutionOptions.native` is `None`, continue
  6. if `native.family` is present and its variant matches the target family, continue
  7. if `native.family` is present and its variant does not match the target family, mark the
     attempt statically incompatible
  8. if `native.provider` is present and its variant matches the resolved provider kind, continue
  9. if `native.provider` is present and its variant does not match the resolved provider kind, mark the
     attempt statically incompatible
  10. apply `CapabilityMismatchPolicy`

  ### Ownership Rule

  The layered native option model is the public counterpart to the internal codec/overlay split:

  - family codecs are responsible for family-scoped option encoding and validation
  - provider overlays are responsible for provider-scoped option encoding and validation

  ### Classification Rule

  A field belongs in `FamilyOptions` when all of the following are true:

  - the field has the same meaning across multiple providers in the same family
  - the field is encoded by the family codec rather than by one provider overlay
  - compatibility should be validated at the family layer rather than the provider layer

A field belongs in `ProviderOptions` when any of the following are true:

- the field is only accepted by one concrete provider
- the field affects provider-specific routing, account, plugin, debug, or endpoint behavior
- the field requires provider-specific request shape, provider-specific dynamic headers, or
  provider-specific validation
- the field is currently exposed by one provider, but its family-shared semantics, validation, or
  encoding are not yet locked strongly enough to promote it into `FamilyOptions`

Provider options may therefore temporarily retain fields that are candidate family-level controls.

Promotion into `FamilyOptions` is intentional once a field is confirmed to:

- have aligned meaning across multiple providers in the family
- belong to family-layer validation
- be encoded by the family codec rather than one provider overlay

## 7. Target

  ### Purpose

  Represents a logical provider/model destination.

  ### Public Shape

  ```rust
  pub struct Target {
      pub instance: ProviderInstanceId,
      pub model: Option<String>,
  }
  ```

  ### Notes

  - Target stays focused on destination identity
  - it names one registered runtime destination instance
  - it does not own layered native request options or execution overrides

  ## 8. AttemptExecutionOptions

  ### Purpose

  Represents attempt-local execution behavior that may vary by target without changing the logical
  call contract.

  ### Public Shape

  ```rust
  pub struct AttemptExecutionOptions {
      pub native: Option<NativeOptions>,
      pub timeout_overrides: TransportTimeoutOverrides,
      pub extra_headers: BTreeMap<String, String>,
  }
  ```

  ### Semantics

  Allowed here:

  - target-scoped native request options:
    - family-scoped native options
    - provider-scoped native options
  - attempt-local timeout overrides:
    - request timeout
    - stream-setup timeout
    - stream-idle timeout
  - attempt-local extra transport headers

  Not allowed here:

  - response mode
  - observer override
  - request_id_header_override
  - route-wide transport options
  - provider/protocol request hints such as expected content type, SSE defaults, or
    preserve-error-body-for-adapter-decode
  - anything that changes the logical call shape

  ### Locked design rule

  Overlap between route-wide transport options, attempt-local transport overrides, and adapter-
  produced protocol options is intentionally minimized.

  This is a design goal, not an implementation accident.

  ## Transport Option Ownership and Merge Rules

  ### Ownership Split

  Transport concerns are intentionally split so that most fields are owned by exactly one layer.

  #### Route-wide ExecutionOptions.transport owns:

  - request-id header override
  - call-wide extra headers

  #### Attempt-local AttemptExecutionOptions owns:

  - attempt-local extra headers
  - attempt-local timeout overrides:
    - request timeout
    - stream-setup timeout
    - stream-idle timeout

  #### Provider adapters own protocol-specific HttpRequestOptions, including:

  - accept
  - expected_content_type
  - preserve_error_body_for_adapter_decode
  - SSE-specific defaults
  - SSE parser limits
  - provider/protocol response framing hints that are not route-owned transport controls

  #### Provider adapters also own provider-generated dynamic headers via provider request planning

  - provider-specific request headers derived from provider-scoped native options or
    endpoint/protocol behavior
  - these headers are part of the adapter-produced provider request plan, not caller-owned transport
    options and not `ResolvedTransportOptions`

  ### Locked non-overlap rules

  The following fields must not be shared across multiple ownership layers:

  - request_id_header_override is route-wide only
  - request_id_header_override affects response request-id extraction only; it is not an outbound
    header layer
  - content-type expectations are adapter-owned only
  - SSE parser limits are adapter-owned only
  - preserve_error_body_for_adapter_decode is adapter-owned only
  - typed timeout fields are runtime-owned only
  - response mode is route-wide only
  - observer override is route-wide only
  - first-byte timeout is not a separate public/runtime-facing field; it remains an internal
    transport behavior governed by `stream_idle_timeout`

  ### Merge behavior

  Where merging is required, the merge rules are:

  #### Headers

  Transport constructs headers from explicit layers in this order:

  1. platform default headers
  2. route-wide extra headers
  3. attempt-local extra headers
  4. adapter/provider-generated dynamic headers
  5. auth headers applied by transport according to auth style

  Later layers override earlier layers on key collision, except auth placement which remains
  transport-controlled.

  `request_id_header_override` is not part of this header merge order.
  It only changes which response header transport reads when extracting `HttpResponseHead.request_id`.

  Runtime does not pre-merge these layers into one opaque header map.
  Runtime passes caller-owned transport headers through `ResolvedTransportOptions` and adapter-owned
  provider headers through the provider request plan / transport execution input.

  #### Timeouts

  Timeouts are resolved in this order:

  1. transport defaults from provider/runtime configuration:
      - request timeout
      - stream-setup timeout
      - stream-idle timeout
  2. attempt-local timeout overrides for those same fields

  No adapter-owned timeout clamping or timeout override step exists.

  ### Timeout Design Decision

  The redesign keeps typed runtime-facing control over:

  - request timeout
  - stream-setup timeout
  - stream-idle timeout

  The redesign does not introduce a separate public/runtime-facing first-byte timeout field.

  First-byte timeout remains internal to transport behavior and is governed by the resolved
  `stream_idle_timeout`.

  ### Architectural rule

  If a new transport field would require open-ended overlap between route-wide transport options,
  attempt-local execution overrides, and adapter request hints, the field should be reassigned to a
  single owning layer instead of expanding precedence rules.

  Typed timeout fields are a locked example of this rule: they are runtime-owned and never adapter-
  owned.

  ## 9. AttemptSpec

  ### Purpose

  Represents one candidate attempt in a route: destination plus target-local overrides.

  ### Public Shape

  ```rust
  pub struct AttemptSpec {
      pub target: Target,
      pub execution: AttemptExecutionOptions,
  }
  ```

  ### Builder Examples

  ```rust
  AttemptSpec::to(Target::new(ProviderInstanceId::new("openai-default")))
  AttemptSpec::to(Target::new(ProviderInstanceId::new("openai-default")).with_model("gpt-5"))
  AttemptSpec::to(Target::new(ProviderInstanceId::new("openrouter-default")))
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
  ```

  ## 10. CapabilityMismatchPolicy

  ### Purpose

  Defines what routing should do when an attempt is statically incompatible with the task before any
  provider request is sent.

  ### Public Shape

  ```rust
  pub enum CapabilityMismatchPolicy {
      FailFast,
      SkipIncompatibleTargets,
  }
  ```

  ### Default

  FailFast

  ### Semantics

  - FailFast: return a planning error immediately when the current attempt is statically incompatible
  - SkipIncompatibleTargets: record the attempt as skipped and advance to the next compatible target

  ### Locked validation scope

  `CapabilityMismatchPolicy` applies only to high-confidence static incompatibilities that runtime
  can determine before execution.

  Those checks are exactly:

  - `ResponseMode::Streaming` requested for a target whose `ProviderCapabilities` do not support
    streaming
  - `NativeOptions.family` variant does not match the selected target family
  - `NativeOptions.provider` variant does not match the resolved provider kind
  - family-scoped native options requested for a target whose provider does not support family-native
    options at all
  - provider-scoped native options requested for a target whose provider does not support provider-
    scoped native options at all

  `CapabilityMismatchPolicy` does not decide model-level or deployment-level feature support for
  request fields such as tools, structured output, `top_p`, stop sequences, reasoning controls, or
  arbitrary passthrough fields.

  ## 11. Attempt Outcome Metadata and Observer Model

  ### Design Decision

  A skipped attempt is a planning outcome, not a failed execution.

  The architecture therefore models route attempt history as a first-class ordered record stream that
  includes both skipped and executed attempts.

  ```rust
  pub enum SkipReason {
      CapabilityMismatch {
          message: String,
      },
  }

  pub enum AttemptDisposition {
      Skipped {
          reason: SkipReason,
      },
      Succeeded {
          status_code: Option<u16>,
          request_id: Option<String>,
      },
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
      pub model: Option<String>,
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
  ```

  ### Locked semantics

  - `ResponseMeta.attempts` is the ordered route attempt history, not just executed attempts
  - skipped attempts are recorded in success metadata when routing later succeeds
  - skipped attempts are recorded in route-planning failures when no compatible attempt succeeds
  - a skipped attempt never has provider request id or executed-attempt status metadata

  ### Observer rule

  Skipped attempts must not emit execution lifecycle events that imply provider execution started.

  The observer model therefore includes:

  ```rust
  pub struct AttemptSkippedEvent {
      pub provider_instance: Option<ProviderInstanceId>,
      pub provider_kind: Option<ProviderKind>,
      pub model: Option<String>,
      pub target_index: Option<usize>,
      pub attempt_index: Option<usize>,
      pub elapsed: Duration,
      pub reason: SkipReason,
  }
  ```

  And observer semantics are locked as:

  - `on_attempt_start` means provider execution is about to begin
  - `on_attempt_success` means an executed attempt succeeded
  - `on_attempt_failure` means an executed attempt failed after execution began
  - `on_attempt_skipped` means planning rejected the attempt before provider execution

  Skipped attempts do not emit:

  - `on_attempt_start`
  - `on_attempt_failure`

  ## 11. FallbackPolicy

  ### Purpose

  Represents only the rules that decide whether routing should continue after a failed attempted
  execution.

  ### Public Shape

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

  ### Notes

  - fallback targets do not live here anymore
  - this type answers only: should we advance to the next attempt?
  - `FallbackPolicy` is rule-driven in the target architecture
  - legacy fallback toggles are migration-only compatibility behavior and are not part of this public
    target shape

  ### Matching Semantics

  `FallbackPolicy` evaluates rules in insertion order.
  The first matching rule decides the fallback outcome.

  `FallbackMatch` uses AND semantics across fields:

  - every non-empty field must match
  - empty fields impose no restriction
  - if no rule matches, fallback does not continue

  Field-specific semantics:

  - `error_kinds` matches `RuntimeError.kind`
  - `status_codes` matches `RuntimeError.status_code`; if the error has no status code, this matcher
    fails
  - `provider_codes` matches only after family + provider error decoding and runtime normalization
    have populated a provider code
  - `provider_codes` are compared after trimming surrounding whitespace
  - blank `provider_codes` rule values never match
  - `provider_kinds` matches the resolved concrete adapter identity for the executed attempt
  - `provider_instances` matches the executed `ProviderInstanceId`

  Action semantics:

  - `FallbackAction::RetryNextTarget` advances to the next attempt already present on `Route`
  - `FallbackAction::Stop` stops fallback evaluation and surfaces the current error

  ### Fallback Evaluation Rule

  `FallbackPolicy` evaluates only runtime-normalized executed failures.
  It does not inspect raw transport responses directly.

  Locked error-normalization flow:

  1. transport executes the request and returns a framed response or a transport-level failure
  2. if the response status is successful, runtime follows the normal success decode path
  3. if the response status is non-success and the adapter request plan enabled
     `preserve_error_body_for_adapter_decode`, runtime preserves the error body for adapter decode
  4. runtime invokes family-codec error decoding to extract shared family-shaped error details
  5. runtime invokes provider-overlay error decoding to refine or override concrete-provider error
     details
  6. runtime merges the decoded error information with provider-overlay fields taking precedence when
     both layers populate the same field
  7. runtime normalizes the merged result into `RuntimeError`, including provider-specific fields such
     as `provider_code` when available
  8. `FallbackPolicy` evaluates `rules` in insertion order against that normalized `RuntimeError`
  9. the first matching rule decides whether routing retries the next target or stops
  10. if the response status is non-success and the adapter request plan did not enable
     `preserve_error_body_for_adapter_decode`, the failure remains transport/status-derived and
     fallback evaluates that normalized transport error

  Consequences:

  - `FallbackRule.provider_codes` is meaningful only when family + provider error decoding and
    runtime normalization populated a provider code
  - fallback never runs against raw HTTP bodies or transport response objects
  - family and provider error-body decoding happen before fallback evaluation whenever the adapter
    requested error-body preservation
  - if no rule matches the normalized error, fallback does not continue

  ## 12. Route

  ### Purpose

  Represents the ordered attempt chain for a logical call.

  ### Public Shape

  ```rust
  pub struct Route {
      pub primary: AttemptSpec,
      pub fallbacks: Vec<AttemptSpec>,
      pub fallback_policy: FallbackPolicy,
      pub capability_mismatch_policy: CapabilityMismatchPolicy,
  }
  ```

  ### Builder Example

  ```rust
  Route::to(
      AttemptSpec::to(Target::new(ProviderInstanceId::new("openai-default")).with_model("gpt-5"))
  )
  .with_fallback(
      AttemptSpec::to(Target::new(ProviderInstanceId::new("openrouter-default")).with_model("openai/gpt-5"))
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
  .with_capability_mismatch_policy(CapabilityMismatchPolicy::FailFast)
  ```

  ## 13. ExecutionPlan

  ### Purpose

  Represents one fully resolved provider execution attempt.

  ### Internal Shape

  ```rust
  pub struct ResolvedProviderAttempt {
      pub instance_id: ProviderInstanceId,
      pub provider_kind: ProviderKind,
      pub family: ProviderFamilyId,
      pub model: String,
      pub native_options: Option<NativeOptions>,
  }

  pub struct ExecutionPlan {
      pub response_mode: ResponseMode,
      pub task: TaskRequest,
      pub provider_attempt: ResolvedProviderAttempt,
      pub platform: PlatformConfig,
      pub auth_token: Option<AuthCredentials>,
      pub transport: ResolvedTransportOptions,
      pub capabilities: ProviderCapabilities,
  }
  ```

  ### Notes

  - this is the core handoff to the adapter and transport path
  - ambiguity is eliminated before adapter planning starts
  - `AttemptSpec` is consumed during planning and does not cross the adapter boundary
  - adapters receive only provider-resolved attempt data, not routing-layer attempt structure
  - the resolved attempt carries both runtime destination identity and adapter identity
  - runtime/provider-instance configuration is consumed before `ExecutionPlan` is created
  - `ExecutionPlan` carries resolved transport-facing provider state, not unresolved provider config

  ## Family and Provider Option Types

  These types make the layered native option contract concrete.

  ## OpenAiCompatibleOptions

  Shared controls for providers in the OpenAI-compatible family.

  ```rust
  pub struct OpenAiCompatibleOptions {
      pub parallel_tool_calls: Option<bool>,
      pub reasoning: Option<OpenAiReasoning>,
  }
  ```

Fields belong here only when they are intentionally shared across OpenAI-compatible providers and
encoded by the OpenAI-compatible family codec.

Fields should be added here conservatively.
If an OpenRouter-exposed field might eventually prove to be shared across OpenAI-compatible
providers, it remains in `OpenRouterOptions` until its family-level semantics, validation, and
encoding are clear enough to lock into this shared surface.

  ## AnthropicFamilyOptions

  Shared controls for the Anthropic family.

  ```rust
  pub struct AnthropicFamilyOptions {
      pub thinking: Option<AnthropicThinking>,
  }
  ```

  This type exists even if it is initially small, because the public model is locked to family-
  scoped plus provider-scoped layering rather than to provider-only option enums.

  ## OpenAiOptions

  Only fields that are truly OpenAI-specific and not part of the shared task surface or the
  OpenAI-compatible family layer.

  ```rust
  pub struct OpenAiOptions {
      pub service_tier: Option<String>,
      pub store: Option<bool>,
  }
  ```

## OpenRouterOptions

Public replacement for the current internal OpenRouterOverrides.

This struct contains OpenRouter-exposed controls that are not part of the shared task surface and
are not yet confirmed members of the locked `OpenAiCompatibleOptions` family surface.

Some fields here are definitively OpenRouter-specific.
Others are intentionally provisional and may later move into `OpenAiCompatibleOptions` once they
are verified to have intentionally shared semantics, validation, and encoding across OpenAI-
compatible providers.

```rust
pub struct OpenRouterOptions {
      pub fallback_models: Vec<String>,
      pub provider_preferences: Option<serde_json::Value>,
      pub plugins: Vec<serde_json::Value>,
      pub frequency_penalty: Option<f32>,
      pub presence_penalty: Option<f32>,
      pub logit_bias: Option<serde_json::Value>,
      pub logprobs: Option<bool>,
      pub top_logprobs: Option<u8>,
      pub seed: Option<i64>,
      pub user: Option<String>,
      pub session_id: Option<String>,
      pub trace: Option<serde_json::Value>,
      pub route: Option<String>,
      pub max_tokens: Option<u32>,
      pub modalities: Option<Vec<String>>,
      pub image_config: Option<serde_json::Value>,
      pub debug: Option<serde_json::Value>,
      pub stream_options: Option<serde_json::Value>,
  }
  ```

  ## AnthropicOptions

  Anthropic-provider-specific fields not represented by the generic task layer or Anthropic family
  layer.

  ```rust
  pub struct AnthropicOptions {
      pub top_k: Option<u32>,
  }
  ```

  ## Native Option Composition Examples

  OpenAI direct attempt with family and provider options:

  ```rust
  NativeOptions {
      family: Some(FamilyOptions::OpenAiCompatible(
          OpenAiCompatibleOptions {
              parallel_tool_calls: Some(true),
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
  }
  ```

  OpenRouter attempt with shared OpenAI-family controls plus OpenRouter-specific routing controls:

  ```rust
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
  ```

  ## Provider Family and Overlay Architecture

  ## ProviderFamilyId

  ```rust
  pub enum ProviderFamilyId {
      OpenAiCompatible,
      Anthropic,
  }
  ```

  ## ProviderKind

  ```rust
  pub enum ProviderKind {
      OpenAi,
      OpenRouter,
      Anthropic,
      GenericOpenAiCompatible,
  }
  ```

  `ProviderKind` identifies the concrete adapter/overlay behavior used for an attempt.

  ## ProviderInstanceId

  ```rust
  pub struct ProviderInstanceId(String);
  ```

  `ProviderInstanceId` identifies one registered runtime destination instance.

  ## RegisteredProvider

  Runtime registration record that ties destination identity to adapter identity and runtime config.

  ```rust
  pub struct RegisteredProvider {
      pub instance_id: ProviderInstanceId,
      pub kind: ProviderKind,
      pub config: ProviderConfig,
  }
  ```

  Locked semantics:

  - routes target `ProviderInstanceId`
  - runtime resolves `ProviderInstanceId` into `RegisteredProvider`
  - adapters are selected by `ProviderKind`
  - family codecs are selected by `ProviderFamilyId` from the resolved provider descriptor
  - multiple registered instances may share the same `ProviderKind` and `ProviderFamilyId`
  - different instances may vary in auth credentials, base URL, default model, retry policy, and
    timeout defaults
  - this is the supported model for self-hosted OpenAI-compatible endpoints that share a family
    codec while remaining distinct runtime destinations

  ## ProviderDescriptor

  Static identity/configuration metadata for a provider.

  ```rust
  pub struct ProviderDescriptor {
      pub kind: ProviderKind,
      pub family: ProviderFamilyId,
      pub default_base_url: &'static str,
      pub endpoint_path: &'static str,
      pub auth_style: AuthStyle,
      pub request_id_header: HeaderName,
      pub default_headers: HeaderMap,
  }
  ```

  ### Purpose

  `ProviderDescriptor` is adapter-owned static metadata.

  It replaces the current pattern where adapters directly synthesize `PlatformConfig` from hardcoded
  provider facts such as default base URL, auth style, request-id header, and default headers.

  In the target architecture:

  - adapters expose `descriptor()`
  - runtime composes the transport-facing `PlatformConfig`
  - adapters do not construct `PlatformConfig` directly
  - descriptors are keyed by `ProviderKind`, not by `ProviderInstanceId`

  ## ProviderConfig

  Runtime/provider-instance configuration for a concrete registered provider instance.

  This is the conceptual successor to today's runtime `ProviderConfig`, which already owns fields such
  as API key, base URL override, default model, retry policy, and timeout defaults.

  ```rust
  pub struct ProviderConfig {
      pub auth_token: Option<AuthCredentials>,
      pub base_url: Option<String>,
      pub default_model: Option<String>,
      pub retry_policy: Option<RetryPolicy>,
      pub request_timeout: Option<Duration>,
      pub stream_setup_timeout: Option<Duration>,
      pub stream_idle_timeout: Option<Duration>,
  }
  ```

  ### Purpose

  `ProviderConfig` is runtime-owned configuration input.

  It represents deployment-specific or user-supplied settings for one provider instance, not static
  facts about the provider protocol.
  Runtime stores this config per `ProviderInstanceId`.

  ## PlatformConfig

  Resolved transport-facing configuration derived by runtime from `ProviderDescriptor` and
  `ProviderConfig`.

  This follows the direction of the current codebase, which already has a transport-facing
  `PlatformConfig`, but moves ownership of its construction from adapters into runtime.

  ```rust
  pub struct PlatformConfig {
      pub protocol: ProtocolKind,
      pub base_url: String,
      pub auth_style: AuthStyle,
      pub request_id_header: HeaderName,
      pub default_headers: HeaderMap,
  }
  ```

  ### Locked composition rule

  Runtime resolves `PlatformConfig` by combining:

  - static adapter-owned `ProviderDescriptor`
  - runtime-owned `ProviderConfig`

  Concretely:

  - descriptor supplies protocol family, default base URL, auth style, request-id header, default
    headers, and default endpoint path
  - provider config supplies base URL override, auth credentials, default model, retry policy, and
    transport defaults
  - `PlatformConfig` intentionally carries the resolved base URL but not the selected endpoint path
  - `PlatformConfig.request_id_header` defines the default response header transport uses to extract
    upstream request ids from responses
  - runtime validates and normalizes the effective base URL and produces the transport-facing
    `PlatformConfig`
  - runtime resolves the effective endpoint path separately using:
      1. `ProviderRequestPlan.endpoint_path_override` when present
      2. otherwise `ProviderDescriptor.endpoint_path`
  - runtime joins the resolved base URL and effective endpoint path to construct the final outbound
    request URL before transport execution

  ## ProviderCapabilities

  ```rust
  pub struct ProviderCapabilities {
      pub supports_streaming: bool,
      pub supports_family_native_options: bool,
      pub supports_provider_native_options: bool,
  }
  ```

  ### Response-mode capability rule

  Non-streaming execution is the baseline contract for providers admitted into this runtime.
  The capability model therefore only needs to express whether streaming is supported as an optional
  additional execution mode.

  Concretely:

  - `ResponseMode::NonStreaming` is not gated by a separate `supports_non_streaming` capability bit
  - `ResponseMode::Streaming` requires `supports_streaming = true`
  - if the caller requests `ResponseMode::Streaming` for a provider that does not support streaming,
    the attempt is statically incompatible
  - runtime must reject or skip that attempt according to `CapabilityMismatchPolicy` before provider
    execution begins

  ### Static capability scope

  `ProviderCapabilities` is intentionally narrow.
  It models only high-confidence invariants that runtime can validate reliably before execution
  across providers.

  Static capability validation is limited to:

  - whether the target supports streaming at all
  - whether the target supports family-scoped native options at all
  - whether the target supports provider-scoped native options at all

  Static capability validation does not attempt to prove model-level or deployment-level support for
  fine-grained request features such as:

  - tools or tool calling
  - structured output
  - `top_p`
  - stop sequences
  - reasoning controls
  - other request knobs whose support may vary by model, deployment, or upstream inference host

  Those features are validated during adapter planning and/or by the upstream provider response path,
  then surfaced through normalized runtime errors rather than `CapabilityMismatchPolicy`.

  ### Layered Native Capability Semantics

  `ProviderCapabilities` participates in native-option validation only at two coarse layers:

  - family-scoped native options are allowed only when the provider exposes family-native-option
    support
  - provider-scoped native options are allowed only when the provider exposes provider-native-option
    support

  ## ProviderFamilyCodec

  Shared request/response/stream behavior for a provider family.

  ```rust
  pub trait ProviderFamilyCodec {
      fn encode_task(
          &self,
          task: &TaskRequest,
          model: &str,
          response_mode: ResponseMode,
          family_options: Option<&FamilyOptions>,
      ) -> Result<EncodedFamilyRequest, AdapterError>;

      fn decode_response(
          &self,
          body: Value,
          format: &ResponseFormat,
      ) -> Result<Response, AdapterError>;

      fn decode_error(
          &self,
          body: &Value,
      ) -> Option<ProviderErrorInfo>;

      fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector>;
  }
  ```

  ### Responsibilities

  - map shared family request fields
  - consume matching family-scoped native options
  - produce the shared family request plan shape before provider-specific overlay augmentation
  - set family-default transport kind, response kind, endpoint override, and protocol-level
    request options consistent with the requested `ResponseMode`
  - map shared family response shape
  - decode shared family-shaped error bodies into `ProviderErrorInfo`
  - supply default family stream projector

  ## EncodedFamilyRequest

  The result of family-level request planning before provider-specific overlay augmentation.

  ```rust
  pub struct EncodedFamilyRequest {
      pub body: Value,
      pub warnings: Vec<RuntimeWarning>,
      pub transport_kind: ProviderTransportKind,
      pub response_kind: ProviderResponseKind,
      pub endpoint_path_override: Option<String>,
      pub provider_headers: HeaderMap,
      pub request_options: HttpRequestOptions,
  }
  ```

  ### Semantics

  - this is the family-level intermediate request plan
  - it is not a provider-specific public request type
  - it is the common conceptual ancestor of today's family-specific encoded request values
  - `endpoint_path_override` is the family-level hook for selecting a non-default endpoint path
  - provider-specific overlays may mutate it before the adapter finalizes `ProviderRequestPlan`
  - `provider_headers` carries adapter-owned dynamic request headers after `AdapterContext`
    metadata-based transport control is retired

  ## ProviderRequestPlan

  Final adapter-produced request contract consumed by runtime when constructing the transport
  execution input.

  ```rust
  pub struct ProviderRequestPlan {
      pub body: Value,
      pub warnings: Vec<RuntimeWarning>,
      pub transport_kind: ProviderTransportKind,
      pub response_kind: ProviderResponseKind,
      pub endpoint_path_override: Option<String>,
      pub provider_headers: HeaderMap,
      pub request_options: HttpRequestOptions,
  }
  ```

  ### Locked rule

  `ProviderRequestPlan` carries adapter-produced dynamic headers explicitly.
  These headers do not get folded into `ResolvedTransportOptions`, and they are not tunneled through
  metadata.

  `ProviderRequestPlan.endpoint_path_override` is the only adapter-controlled endpoint-selection
  mechanism.
  When present, it replaces `ProviderDescriptor.endpoint_path`.
  Adapters do not construct the final URL; runtime does.

  ## ProviderOverlay

  Provider-specific augmentation for a concrete provider in a family.

  ```rust
  pub trait ProviderOverlay {
      fn apply_provider_overlay(
          &self,
          request: &mut EncodedFamilyRequest,
          provider_options: Option<&ProviderOptions>,
      ) -> Result<(), AdapterError>;

      fn decode_provider_error(&self, body: &Value) -> Option<ProviderErrorInfo>;

      fn decode_response_override(
          &self,
          body: Value,
          requested_format: &ResponseFormat,
      ) -> Option<Result<Response, AdapterError>>;

      fn create_stream_projector_override(&self) -> Option<Box<dyn ProviderStreamProjector>>;
  }
  ```

  ## ProviderErrorInfo

  Shared intermediate error details extracted from an error response body before runtime
  normalization and fallback evaluation.

  ```rust
  pub struct ProviderErrorInfo {
      pub provider_code: Option<String>,
      pub message: Option<String>,
      pub kind: Option<RuntimeErrorKind>,
  }
  ```

  ### Responsibilities

  - apply provider-scoped request-body controls
  - consume matching provider-scoped native options
  - adjust provider-specific protocol request hints such as `HttpRequestOptions`
  - emit provider-generated dynamic headers when required by the provider protocol or selected
    native options
  - adjust endpoint override when required by a concrete provider
  - decode provider-specific error bodies into `ProviderErrorInfo`
  - refine or override family-decoded error details for concrete-provider semantics
  - override family decode/projector behavior if needed for provider quirks

  ### Composition Model

  - OpenAI provider kind = OpenAI-compatible family codec + OpenAI overlay
  - OpenRouter provider kind = OpenAI-compatible family codec + OpenRouter overlay
  - Anthropic provider kind = Anthropic family codec + Anthropic overlay
  - GenericOpenAiCompatible provider kind = OpenAI-compatible family codec + generic OpenAI-
    compatible overlay

  Error decoding follows the same layered composition:

  - family codec decodes shared family-shaped error bodies first
  - provider overlay decodes and refines provider-specific error details second
  - runtime merges those results and normalizes them into `RuntimeError` before fallback evaluation

  ## Adapter Boundary Redesign

  ## Current Problem

  ProviderAdapter::plan_request(req) is not target-aware and cannot plan layered native attempt data
  cleanly.

  ## New Internal Adapter Contract

  ```rust
  pub trait ProviderAdapter {
      fn kind(&self) -> ProviderKind;
      fn descriptor(&self) -> &ProviderDescriptor;
      fn capabilities(&self) -> &ProviderCapabilities;

      fn plan_request(
          &self,
          execution: &ExecutionPlan,
      ) -> Result<ProviderRequestPlan, AdapterError>;

      fn decode_response_json(
          &self,
          body: Value,
          requested_format: &ResponseFormat,
      ) -> Result<Response, AdapterError>;

      fn create_stream_projector(&self) -> Box<dyn ProviderStreamProjector>;
  }
  ```

  ## Adapter Planning Flow

  1. read execution.task
  2. read layered native options from `execution.provider_attempt.native_options`
  3. encode shared family request through the family codec into `EncodedFamilyRequest`, passing the
     matching family-scoped options
  4. apply provider overlay to the family request plan, passing the matching provider-scoped options
  5. produce `ProviderRequestPlan`

  ## Provider Adapter vs Runtime vs Transport Responsibilities

  ### Runtime owns

  - converting MessageCreateInput into TaskRequest
  - constructing Route
  - constructing ExecutionOptions
  - resolving `Target.instance` into a `RegisteredProvider`
  - resolving an AttemptSpec into a `ResolvedProviderAttempt`
  - assembling `ExecutionPlan` from task, resolved provider attempt, provider config, capabilities,
    and resolved transport options
  - selecting adapter by `ProviderKind`
  - selecting provider config and auth credentials from the resolved registered instance
  - resolving `PlatformConfig` from `ProviderDescriptor` + `ProviderConfig`
  - validating static capabilities
  - applying CapabilityMismatchPolicy
  - normalizing route-wide and attempt-local transport inputs into the transport request contract

  ### Provider adapter owns

  - converting ExecutionPlan into a provider request plan
  - using family codecs for shared family mapping
  - consuming matching family-scoped native options in the family codec
  - using provider overlays for provider-specific request augmentation
  - consuming matching provider-scoped native options in the provider overlay
  - finalizing `ProviderRequestPlan` from the overlaid family request plan
  - decoding successful provider responses
  - exposing family-level and provider-level error decode hooks when runtime preserves an error
    response body for adapter decode
  - selecting non-default endpoint paths only by producing `endpoint_path_override`
  - creating or overriding stream projectors where needed

  ### Transport owns

  - executing final outbound requests from typed transport inputs
  - materializing headers
  - placing auth credentials according to auth style
  - applying transport retries before body handoff
  - enforcing request and stream timeouts
  - executing JSON, bytes, and SSE requests
  - returning low-level framed responses and transport/status failures without interpreting
    provider-specific error bodies

  ### Locked rule

  Provider adapters do not consume route-wide transport options directly.
  Provider adapters emit protocol-specific request hints.
  Runtime merges those hints with typed route/attempt transport settings before transport execution.

  More specifically:

  - family codecs produce the initial protocol-level request hints in `EncodedFamilyRequest`
  - family codecs consume only family-scoped native options
  - provider overlays consume only provider-scoped native options
  - provider overlays may modify those hints for concrete provider behavior
  - adapters finalize that result into `ProviderRequestPlan`
  - runtime does not invent provider/protocol hints on behalf of adapters
  - runtime evaluates fallback only after normalizing an executed failure into `RuntimeError`
  - runtime passes adapter-produced provider headers through the normalized transport execution input
    without reclassifying them as runtime-owned transport options
  - transport merges adapter-produced provider headers with route-owned headers using the locked
    header precedence order
  - transport does not interpret provider-specific error bodies for fallback purposes
  - runtime orchestrates family-codec decode followed by provider-overlay decode before fallback
    evaluation when the adapter requested error-body preservation
  - adapters do not consume `AttemptSpec`; they consume `ResolvedProviderAttempt` through
    `ExecutionPlan`
  - adapters expose static descriptor data keyed by `ProviderKind`; runtime resolves the transport-
    facing platform for the selected `ProviderInstanceId`

  ## High-Level Public API Design

  ## MessageCreateInput

  Remains the ergonomic high-level task builder and normalizes into TaskRequest.

  ```rust
  pub struct MessageCreateInput {
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
  ```

  ### Rule

  No provider-specific fields are added to the generic task builder.
  Provider payload metadata belongs on `TaskRequest` / `MessageCreateInput`, not on `Route` or
  `ExecutionOptions`.

  ## Direct Concrete Clients

  ### Goal

  Preserve existing ergonomics by automatically creating a single-attempt route.

  ### High-Level Usage

  ```rust
  openai_client.messages().create(input).await?;
  openai_client.messages().model("gpt-5").create(input).await?;
  openai_client.streaming().create(input).await?;
  openrouter_client.messages().create(input).await?;
  ```

  ### Internal Normalization

  A direct client call normalizes into:

  - TaskRequest
  - Route with one AttemptSpec targeting the client's registered `ProviderInstanceId`
  - inferred ExecutionOptions

  ### Direct Native Ergonomics

  Concrete clients may expose native-option convenience overloads.

  Examples:

  ```rust
  openrouter_client
      .messages()
      .create_with_options(
          input,
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
          },
      )
      .await?;

  openai_client
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

  ### Decision

  Native option convenience lives on concrete client APIs, not on MessageCreateInput.

  Direct clients also preserve per-call model override ergonomics on the fluent call surface.

  Example:

  ```rust
  openai_client
      .messages()
      .model("gpt-5")
      .create(input)
      .await?;
  ```

  Locked rule:

  - direct clients may expose `.model(...)`-style fluent configuration
  - such fluent model selection does not mutate `TaskRequest`
  - direct clients internally normalize fluent model selection into the single generated
    `AttemptSpec.target.model`
  - direct clients resolve their configured registered instance and target that instance in the
    generated route

  ## AgentToolkit

  ### Goal

  Remain the primary routed multi-provider entry point.

  ### High-Level Usage

  ```rust
  toolkit.messages().create(input, route).await?;
  toolkit.messages().create_with_meta(input, route).await?;
  toolkit.streaming().create(input, route).await?;
  ```

  ### Semantics

  - .messages() implies ExecutionOptions { response_mode: NonStreaming, ... }
  - .streaming() implies ExecutionOptions { response_mode: Streaming, ... }
  - these ergonomic routed methods infer default `ExecutionOptions`:
      - `observer: None`
      - `transport: TransportOptions::default()`

  ### Explicit Routed Execution Overrides

  Routed APIs that need per-call observer or transport overrides must accept explicit
  `ExecutionOptions`.

  ```rust
  toolkit.messages()
      .create_with_execution(input, route, execution_options)
      .await?;

  toolkit.messages()
      .create_with_meta_and_execution(input, route, execution_options)
      .await?;

  toolkit.streaming()
      .create_with_execution(input, route, execution_options)
      .await?;
  ```

  ### Locked API rule

  - `Route` remains routing-only
  - route-wide observer and transport overrides live on `ExecutionOptions`
  - ergonomic `(input, route)` methods use inferred default `ExecutionOptions`
  - explicit routed methods accept `(input, route, execution_options)` when per-call execution
    overrides are needed

  ## Low-Level Explicit API

  ### Direct Client Low-Level

  ```rust
  client.messages()
      .create_task(task, execution_options)
      .await?;
  ```

  or with an explicit one-off model override:

  ```rust
  client.messages()
      .create_task_with_model(task, "gpt-5", execution_options)
      .await?;
  ```

  or native-option low-level:

  ```rust
  client.messages()
      .create_task_with_attempt(task, attempt_execution_options, execution_options)
      .await?;
  ```

  ### Toolkit Low-Level

  ```rust
  toolkit.messages()
      .create_task(task, route, execution_options)
      .await?;
  ```

  Routed explicit execution control is the public replacement for the execution-oriented behavior that
  currently lives on `SendOptions`.

  ### Replacement Decision

  The current low-level Request API is replaced by:

  - TaskRequest
  - Route
  - ExecutionOptions

  No monolithic replacement Request type remains as the canonical model.

  ## Routing Behavior and Capability Validation

  ## Static Capability Mismatch

  ### Definition

  A target is statically incompatible if the runtime can determine before sending any provider request
  that the attempt violates one of the locked high-confidence invariants.

  Examples:

  - streaming requested but target does not support streaming
  - family-scoped native option requested for a different family than the selected target family
  - provider-scoped native option requested for a different provider than the selected target
  - family-scoped native options requested for a provider that does not support family-native options
    at all
  - provider-scoped native options requested for a provider that does not support provider-native
    options at all

  Not included in static capability mismatch by default:

  - tools or tool calling
  - structured output
  - `top_p`
  - stop sequences
  - reasoning controls
  - model- or deployment-specific request knobs

  ### Default Behavior

  CapabilityMismatchPolicy::FailFast

  Meaning:

  - the current attempt is rejected before transport
  - fallback rules are not evaluated because no provider call was attempted
  - the route is considered invalid for the current attempt

  ### Optional Behavior

  CapabilityMismatchPolicy::SkipIncompatibleTargets

  Meaning:

  - incompatible attempts are recorded as `AttemptRecord { disposition: Skipped { .. } }`
  - `on_attempt_skipped` is emitted for each skipped attempt
  - routing advances to the next compatible attempt
  - if no compatible attempts remain, return a route-planning failure summarizing all skipped attempts

  ## Real-Time Feature Validation and Error Normalization

  Features whose support may vary by model, deployment, or upstream inference host are not treated as
  static capability mismatches in the baseline architecture.

  These include, for example:

  - tools or tool calling
  - structured output
  - `top_p`
  - stop sequences
  - reasoning controls
  - provider passthrough

  Locked behavior:

  1. runtime allows these request features through planning unless they violate one of the explicit
     static mismatch rules above
  2. adapter planning performs deterministic request-shape validation that can be checked locally
  3. upstream provider responses may reject the request for model-level or deployment-level reasons
  4. runtime normalizes those adapter/upstream failures into `RuntimeError`
  5. callers receive a robust normalized error surface even when static capability checking could not
     decide the feature in advance

  This architecture prefers conservative static validation plus strong runtime error normalization
  over inaccurate provider-level capability claims for model-specific features.

  ## Static Capability Mismatch and Fallback

  ### Locked behavior

  Static capability mismatch and fallback are separate concepts.

  - static capability mismatch is determined before any provider request is sent
  - fallback is evaluated only after an attempted provider execution fails

  ### Default

  CapabilityMismatchPolicy::FailFast

  Meaning:

  - if the selected attempt is statically incompatible with the TaskRequest, planning fails
    immediately
  - fallback rules are not consulted because no provider attempt occurred

  ### Optional mode

  CapabilityMismatchPolicy::SkipIncompatibleTargets

  Meaning:

  - statically incompatible attempts are recorded as `AttemptRecord { disposition: Skipped { .. } }`
  - `on_attempt_skipped` is emitted for each skipped attempt
  - routing advances to the next compatible attempt
  - if no compatible attempts remain, return a route-planning failure summarizing skipped attempts

  ### Executed-failure fallback rule

  After an executed attempt fails and runtime normalizes the failure into `RuntimeError`:

  - `FallbackPolicy.rules` are evaluated in insertion order
  - the first matching rule decides the outcome
  - `RetryNextTarget` advances to the next attempt already present on `Route`
  - `Stop`, or no matching rule, stops routing and surfaces the current error

  ### Locked rule

  Transport retry happens within a single ExecutionPlan.
  Route fallback happens between AttemptSpecs.
  These are distinct layers and must remain distinct in the architecture.

  ## Attempt-Local Execution Override Policy

  ### Allowed Target-Local Overrides

  - layered native request options:
    - family-scoped native options
    - provider-scoped native options
  - timeout override
  - extra attempt-specific headers

  ### Disallowed Target-Local Overrides

  - response mode
  - observer override
  - route-wide transport options
  - anything that changes the logical response contract

  ### Architectural Reason

  One logical call may vary destination and local tuning per attempt, but may not vary its global
  execution contract.

  ## End-to-End Information Flow

  ## Direct Client Flow

  1. user constructs MessageCreateInput
  2. direct client converts it into TaskRequest
  3. direct client creates a single AttemptSpec:
      - target instance fixed by client configuration
      - provider kind fixed by client type
      - model from client default and/or explicit fluent override such as `.model(...)`
      - layered native attempt options from direct-client convenience calls if present
  4. direct client wraps that in a single-attempt Route
  5. direct client creates ExecutionOptions from .messages() or .streaming()
  6. planner resolves a concrete ExecutionPlan
  7. adapter uses family codec + provider overlay to build ProviderRequestPlan
  8. runtime builds `TransportExecutionInput` from `ProviderRequestPlan` plus typed transport
     options
  9. agent-transport executes the request
  10. provider response is decoded into canonical response or canonical stream events

  ## Routed Toolkit Flow

  1. user constructs MessageCreateInput
  2. user constructs Route
  3. toolkit converts input into TaskRequest
  4. toolkit resolves `ExecutionOptions`:
      - from inferred defaults for `(input, route)` methods
      - or from explicit caller-supplied `execution_options` for routed override methods
  5. toolkit iterates ordered attempts from the route
  6. for each attempt:
      - resolve target instance into `RegisteredProvider`
      - resolve effective model
      - resolve adapter by provider kind
      - resolve provider config from the registered instance
      - resolve `PlatformConfig` from `ProviderDescriptor` + `ProviderConfig`
      - validate static capabilities
      - apply CapabilityMismatchPolicy
      - if statically incompatible and skipping is enabled:
          - record `AttemptRecord { disposition: Skipped { .. } }`
          - emit `on_attempt_skipped`
          - continue
      - build `ResolvedProviderAttempt`
      - build ExecutionPlan
      - adapter plans provider request
      - runtime builds `TransportExecutionInput` from adapter output plus typed transport inputs
      - transport materializes final headers from explicit header layers
      - transport executes provider request, including any transport-level retries within the attempt
  7. if attempt fails after execution, runtime evaluates `FallbackPolicy.rules` against the
     normalized `RuntimeError`
  8. on success, `ResponseMeta` records selected provider instance, provider kind, model, and the
     ordered `AttemptRecord` history, including prior skips

  ## Internal Planning Pipeline

  ### Proposed Stages

  1. MessageCreateInput -> TaskRequest
  2. direct-client convenience or toolkit route construction
  3. TaskRequest + Route + ExecutionOptions -> AttemptCursor
  4. AttemptCursor -> `RegisteredProvider`
  5. `RegisteredProvider` + effective model + native options -> `ResolvedProviderAttempt`
  6. `ProviderDescriptor` + `ProviderConfig` -> `PlatformConfig`
  7. ResolvedProviderAttempt + TaskRequest + PlatformConfig + auth + ExecutionOptions -> ExecutionPlan
  8. ExecutionPlan -> ProviderRequestPlan
  9. ProviderRequestPlan + ResolvedTransportOptions + platform/auth -> TransportExecutionInput
  10. transport execution
  11. provider decode / stream projection
  12. canonical response and attempt metadata emission

  ## Runtime/Transport Boundary

  ### Public Runtime Side

  ExecutionOptions.transport: TransportOptions

  ### Internal Boundary

  Normalize runtime transport options and provider request plan into a typed agent-transport request
  structure such as `TransportExecutionInput`.

  ### Rule

  No transport control is tunneled through generic metadata at this boundary.
  Provider adapters emit protocol-specific request hints, but they do not directly control route-
  level transport ownership.

  Concretely:

  - provider adapters produce request bodies and protocol-specific `HttpRequestOptions`
  - provider adapters may also produce provider-generated dynamic headers as part of provider
    request planning
  - runtime resolves the effective endpoint path, constructs the final method + URL, and combines
    provider request planning output with resolved transport ownership data into one typed transport
    execution input
  - transport receives one fully normalized request contract
  - transport constructs final outbound headers from explicit header layers instead of metadata-driven
    conventions
  - transport returns framed responses or transport/status failures without decoding provider-specific
    error bodies
  - runtime invokes family-codec error decode followed by provider-overlay error decode before
    fallback evaluation when the adapter enabled `preserve_error_body_for_adapter_decode`
  - the runtime-facing typed timeout model includes request, stream-setup, and stream-idle timeouts
  - first-byte timeout remains transport-internal and is not a separate runtime-facing field

  Proposed normalized flow:

  1. runtime resolves route-wide and attempt-local transport settings into `ResolvedTransportOptions`
  2. adapter produces `ProviderRequestPlan { body, provider_headers, request_options, ... }`
  3. runtime resolves effective endpoint path from:
      - `ProviderRequestPlan.endpoint_path_override` when present
      - otherwise `ProviderDescriptor.endpoint_path`
  4. runtime joins `PlatformConfig.base_url` with the effective endpoint path and builds
     `TransportExecutionInput` with:
      - `platform`
      - `auth_token`
      - `method`
      - `url`
      - `body`
      - `request_options`
      - `transport`
      - `provider_headers`
  5. transport constructs final headers in locked order:
      - platform default headers
      - route-wide extra headers
      - attempt-local extra headers
      - adapter/provider-generated headers
      - auth headers
  6. transport executes the request and extracts response metadata using
     `request_id_header_override` if present, otherwise `PlatformConfig.request_id_header`

  ## Migration and Compatibility

  ## Breaking Changes Allowed

  The redesign may:

  - replace the current semantic role of Request with TaskRequest
  - remove model_id and stream from request-like public types
  - retire SendOptions
  - remove fallback targets from FallbackPolicy
  - replace Target-only routing with AttemptSpec-based routing
  - replace provider-kind-only target identity with instance-scoped target identity
  - change adapter plan methods to consume ExecutionPlan
  - replace adapter-owned `platform_config(base_url)` construction with runtime-owned
    `ProviderDescriptor` + `ProviderConfig` -> `PlatformConfig` composition

  ## Compatibility Strategy

  Temporary compatibility shims are acceptable during migration, but they are not the target
  architecture.

  Possible shims:

  - old Request normalized into TaskRequest + single-attempt Route + ExecutionOptions
  - old SendOptions normalized into Route + ExecutionOptions
  - old provider-kind-targeted routes normalized into instance-targeted routes through a temporary
    runtime lookup shim, but that is not the target architecture
  - old fallback toggles such as `retry_on_status_codes`, `retry_on_transport_error`, and
    `FallbackMode` normalized into equivalent ordered `FallbackRule`s during migration, but those are
    not the target architecture
  - adapters may temporarily retain internal helpers equivalent to today's `platform_config(base_url)`
    while runtime-owned `ProviderDescriptor` composition is introduced, but that is not the target
    architecture

  These shims may be deprecated and later removed.

  ## Tests and Examples

  ## Fixture Tests

  Fixture tests remain authoritative and must continue to pass.
  Allowed changes:

  - update builders and helpers to create TaskRequest, Route, ExecutionOptions, and ExecutionPlan
  - preserve real provider payload expectations

  ## Live Tests

  Live tests must continue to pass.
  They should be updated to prove:

  - direct client usage still works ergonomically
  - routed toolkit fallback still works
  - family-shared behavior still works
  - layered native attempt options are applied correctly

  ## Required Coverage Additions

  - direct OpenAI request with family-scoped options only
  - direct OpenAI request with both family-scoped and OpenAI-specific options
  - direct OpenRouter request with both OpenAI-family and OpenRouter-specific options
  - routed fallback across two registered self-hosted `GenericOpenAiCompatible` instances
  - routed OpenAI primary plus OpenRouter fallback with different attempt-local layered native
    options
  - routed mixed-family fallback path
  - tests proving `Target.instance` resolves to the intended registered provider instance before
    adapter planning
  - tests proving adapter lookup uses `ProviderKind` while config/auth lookup uses
    `ProviderInstanceId`
  - tests proving returned `ResponseMeta` and `AttemptRecord` values include both provider instance
    and provider kind
  - agent-transport header tests updated to validate typed request-id override and typed extra header
    inputs instead of AdapterContext.metadata magic keys
  - runtime/provider integration tests proving that adapter-produced HttpRequestOptions merge
    correctly with route-wide and attempt-local typed transport inputs
  - runtime/provider integration tests proving final URL construction uses
    `ProviderRequestPlan.endpoint_path_override` when present and `ProviderDescriptor.endpoint_path`
    otherwise
  - runtime/provider integration tests proving `ProviderConfig.base_url` override is joined with the
    resolved effective endpoint path before transport execution
  - runtime/provider integration tests proving adapter-produced dynamic headers merge in the locked
    header-precedence order
  - runtime/provider integration tests proving non-success JSON responses are decoded into
    provider-specific `RuntimeError` values before fallback evaluation when
    `preserve_error_body_for_adapter_decode` is enabled
  - tests confirming family codec error decoding runs before provider overlay error decoding and that
    provider-overlay fields take precedence on collision
  - tests confirming `FallbackRule.provider_codes` matches only after family + provider error
    decoding and runtime normalization have populated a provider code
  - tests confirming `FallbackPolicy.rules` are evaluated in insertion order and first match wins
  - tests confirming `FallbackMatch` uses AND semantics across all non-empty fields
  - tests confirming fallback rules can match `ProviderKind`
  - tests confirming fallback rules can match `ProviderInstanceId`
  - tests confirming `FallbackAction::Stop` prevents fallback
  - tests confirming that no matching fallback rule prevents fallback
  - tests confirming blank `provider_codes` rule values do not match
  - static capability mismatch with FailFast for each locked high-confidence invariant
  - static capability mismatch with SkipIncompatibleTargets for each locked high-confidence invariant
  - tests confirming that CapabilityMismatchPolicy::FailFast does not invoke fallback
  - tests confirming that CapabilityMismatchPolicy::SkipIncompatibleTargets records skipped attempts
    and continues routing
  - tests confirming skipped attempts appear in returned `ResponseMeta.attempts`
  - tests confirming skipped attempts emit `on_attempt_skipped` and do not emit
    `on_attempt_start` / `on_attempt_failure`
  - adapter-planning and upstream-error normalization coverage for tools, structured output, `top_p`,
    stop sequences, reasoning controls, and passthrough / `extra` fields
  - tests confirming that transport retries occur within one attempt and do not emit fallback
    behavior on their own
  - non-streaming and streaming parity across the new planning boundary

  ## Final Design Decisions Locked In

  - Request is replaced conceptually by TaskRequest + Route + ExecutionOptions
  - model lives on the attempt target path, not the task
  - stream is replaced by ExecutionOptions.response_mode
  - SendOptions is retired and replaced by Route plus ExecutionOptions
  - observer injection lives on ExecutionOptions
  - routed ergonomic methods may infer default `ExecutionOptions`, but explicit routed APIs accept
    `(input, route, execution_options)` for per-call observer and transport overrides
  - transport options are typed and explicit, not a generic metadata map
  - agent-transport remains the transport implementation boundary; it is not being replaced, but
    its runtime-facing request inputs are made typed and explicit
  - AdapterContext is retired from the long-term transport request contract
  - metadata-based transport override conventions are removed from the target architecture
  - typed timeout fields are runtime-owned end-to-end and are never clamped or overridden by
    adapters
  - overlap between route-wide transport options, attempt-local execution overrides, and adapter
    protocol hints is intentionally minimized by design
  - route-wide transport options own only call-wide transport concerns
  - attempt-local execution overrides own only attempt-local transport concerns, including typed
    request, stream-setup, and stream-idle timeout overrides
  - provider adapters own only protocol-specific HttpRequestOptions
  - adapter-controlled preservation of non-success response bodies for provider error decoding is a
    protocol-specific `HttpRequestOptions` concern
  - provider adapters may also emit provider-generated dynamic headers through provider request
    planning; those headers are carried separately from `ResolvedTransportOptions`
  - runtime owns normalization of typed transport inputs before calling transport
  - runtime resolves the effective endpoint path using `ProviderRequestPlan.endpoint_path_override`
    when present and `ProviderDescriptor.endpoint_path` otherwise
  - runtime constructs the final outbound method + URL before calling transport
  - transport owns final header materialization and merges platform defaults, route-wide headers,
    attempt-local headers, adapter/provider headers, and auth in that order
  - transport receives a fully resolved URL and does not invent endpoint paths
  - `request_id_header_override` overrides response request-id extraction only and does not
    materialize an outbound request header
  - fallback is evaluated only after runtime normalizes an executed failure into `RuntimeError`
  - `FallbackPolicy` is rule-driven in the target architecture
  - `FallbackPolicy.rules` are evaluated in insertion order and first match wins
  - `FallbackMatch` uses AND semantics across all non-empty fields
  - `FallbackAction::RetryNextTarget` advances to the next attempt already present on `Route`
  - `FallbackAction::Stop`, or no matching rule, stops routing and surfaces the current error
  - fallback rules may match by `ProviderKind` and/or `ProviderInstanceId`
  - legacy fallback toggles such as `retry_on_status_codes`, `retry_on_transport_error`, and
    `FallbackMode` are migration-only compatibility behavior, not target architecture
  - family-codec error decoding runs before provider-overlay error decoding when the adapter
    requested error-body preservation
  - provider-overlay error decoding may refine or override family-decoded fields before runtime
    normalization
  - transport does not interpret provider-specific error bodies for fallback purposes
  - `ProviderDescriptor` is adapter-owned static metadata
  - `ProviderKind` identifies concrete adapter and overlay behavior
  - `ProviderInstanceId` identifies one registered runtime destination
  - routes target `ProviderInstanceId`, not `ProviderKind`
  - runtime resolves `ProviderInstanceId` into a registered provider carrying `ProviderKind` and
    `ProviderConfig`, then resolves `ProviderFamilyId` from the selected provider descriptor
  - `ProviderConfig` is runtime-owned provider-instance configuration
  - `PlatformConfig` is the resolved transport-facing result of `ProviderDescriptor` +
    `ProviderConfig`
  - runtime, not adapters, composes `PlatformConfig` in the target architecture
  - `AttemptSpec` is a routing-layer type only and is consumed before adapter planning
  - `ExecutionPlan` carries `ResolvedProviderAttempt`, not `AttemptSpec`
  - first-byte timeout remains transport-internal and is governed by stream-idle timeout behavior
  - transport retry and routed fallback remain separate mechanisms with separate ownership
  - non-streaming mode and streaming mode are strictly separate public execution contracts
  - streaming mode may finalize into a completed canonical response after stream completion
  - non-streaming mode does not internally open SSE streams and finalize them
  - non-streaming execution is the baseline provider contract; streaming is the optional additional
    capability checked during static capability validation
  - static capability mismatch is intentionally narrow and limited to locked high-confidence
    invariants
  - model-level or deployment-level feature support is not inferred from provider-level static
    capabilities in the baseline architecture
  - fine-grained request features such as tools, structured output, `top_p`, stop sequences,
    reasoning controls, and passthrough / `extra` fields are validated during adapter planning and/or
    through normalized upstream errors rather than `CapabilityMismatchPolicy`
  - fallback targets live on Route, not FallbackPolicy
  - layered native options are target-scoped through AttemptSpec
  - native options are layered into family-scoped and provider-scoped parts through `NativeOptions`
  - family-scoped native options must match the attempt target family
  - provider-scoped native options must match the resolved provider kind
  - non-matching native options are static capability mismatches, not ignored inputs
  - family codecs consume family-scoped native options
  - provider overlays consume provider-scoped native options
  - `ProviderRequestPlan.endpoint_path_override` is the only adapter-controlled endpoint-selection
    mechanism
  - attempt-local execution overrides are allowed, but only for attempt-local behavior
  - direct concrete clients preserve `.model(...)`-style per-call ergonomics by normalizing model
    selection into their generated single-attempt route targeting their configured registered
    instance
  - multiple registered instances may share the same `ProviderKind` and `ProviderFamilyId`
  - `GenericOpenAiCompatible` is a first-class provider kind for self-hosted OpenAI-compatible
    endpoints
  - static capability mismatch fails fast by default
  - optional skip-on-mismatch is supported through CapabilityMismatchPolicy
  - skipped attempts are first-class `AttemptRecord` outcomes, not failed executions
  - skipped attempts emit `on_attempt_skipped`, not `on_attempt_start` / `on_attempt_failure`
  - returned route metadata is ordered attempt history, including both skipped and executed attempts,
    and records both provider instance and provider kind
  - provider families are modeled explicitly via codecs; provider-specific differences are modeled via
    overlays

  ## Assumptions

  - preserving architectural clarity is more important than preserving every old surface exactly
  - direct-client ergonomics should remain simple even if the internal model becomes more explicit
  - target-local layered native request controls are essential for robust multi-provider routing
  - the agent-transport crate should receive typed transport intent, not a metadata protocol
  - fixture and live tests form part of the architecture contract and must remain green under the
    redesign
