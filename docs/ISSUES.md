• 1. High: Transport request construction does not have a single owner for HTTP
     method or response framing.

  Why it is a design problem: the spec makes TransportExecutionInput require method
  and response_mode, says runtime constructs the final method + URL, and separately
  says family codecs/adapters emit transport_kind, response_kind, and
  HttpRequestOptions. But neither EncodedFamilyRequest nor ProviderRequestPlan
  carries method, and the later normalized flow omits response_mode entirely. An
  implementer still has to invent whether method is always POST, derived from
  adapter hints, or owned somewhere else, and likewise how transport chooses JSON vs
  bytes vs SSE.

  What decision is still missing or inconsistent: the spec needs one locked source
  of truth for HTTP method and one locked mapping from ResponseMode plus adapter
  plan output into the transport’s response/framing mode.

  Exact section references: “Long-Term Transport Contract” REFACTOR2.md:354,
  “ProviderFamilyCodec” responsibilities REFACTOR2.md:1554, “EncodedFamilyRequest”
  REFACTOR2.md:1565, “ProviderRequestPlan” REFACTOR2.md:1591, “Runtime/Transport
  Boundary” REFACTOR2.md:2220

  2. High: Adapter-planning validation failures are not classified anywhere in
     routing.

  Why it is a design problem: the spec explicitly says adapter planning performs
  deterministic local validation, and that runtime normalizes adapter/upstream
  failures into RuntimeError. But fallback is locked to “executed failures” only,
  while static mismatch is locked to a narrow pre-execution set. A failure returned
  from ProviderAdapter::plan_request happens after attempt resolution but before
  transport, so it is neither a static mismatch nor an executed failure under the
  current rules.

  What decision is still missing or inconsistent: the spec needs to lock whether
  adapter-planning validation failures stop the route immediately, count as
  skippable incompatibilities, or are normalized and evaluated by fallback, and how
  they appear in AttemptRecord and observer events.

  Exact section references: “New Internal Adapter Contract” REFACTOR2.md:1689,
  “Adapter Planning Flow” REFACTOR2.md:1712, “Real-Time Feature Validation and Error
  Normalization” REFACTOR2.md:2057, “Fallback Evaluation Rule” REFACTOR2.md:1065,
  “Static Capability Mismatch and Fallback” REFACTOR2.md:2084, “Routed Toolkit Flow”
  REFACTOR2.md:2171

  3. High: Routed streaming fallback semantics are not locked once a stream has
     started.

  Why it is a design problem: the spec supports toolkit.streaming().create(input,
  route) and defines fallback at the route level, but never states whether fallback
  is allowed after any stream event has been emitted. Retrying another target after
  partial output would materially change the public stream contract and the meaning
  of stream finalization.

  What decision is still missing or inconsistent: the spec needs to decide whether
  fallback in ResponseMode::Streaming is forbidden, allowed only before first
  emitted event, or allowed with a specific replay/termination contract.

  Exact section references: “ResponseMode” REFACTOR2.md:426, “Fallback Evaluation
  Rule” REFACTOR2.md:1065, “AgentToolkit” high-level usage REFACTOR2.md:1912,
  “Routed Toolkit Flow” REFACTOR2.md:2171

  4. High: OpenRouterOptions reintroduces task/execution semantics into provider-
     native options.

  Why it is a design problem: the spec says semantic request content and shared
  generation controls belong on TaskRequest, and execution mode belongs on
  ExecutionOptions. But OpenRouterOptions includes max_tokens and stream_options,
  which overlap directly with TaskRequest.max_output_tokens and streaming behavior.
  The document also describes OpenRouterOptions as outside the shared task surface,
  which contradicts those fields’ semantics.

  What decision is still missing or inconsistent: the spec needs to lock whether
  these fields are aliases, provider-specific augmentations with explicit precedence
  rules, or invalid whenever the corresponding task/execution field is also set.

  Exact section references: “Rule 1” REFACTOR2.md:89, “Task Layer” REFACTOR2.md:167,
  “TaskRequest” REFACTOR2.md:397, “ResponseMode” REFACTOR2.md:426, “NativeOptions”
  classification rule REFACTOR2.md:625, “OpenRouterOptions” REFACTOR2.md:1227

  5. Medium: Request-id extraction is scoped inconsistently with instance-based
     routing.

  Why it is a design problem: the default request-id header is static on
  ProviderDescriptor, which is keyed by ProviderKind, while the only override is
  ExecutionOptions.transport.request_id_header_override, which is route-wide and
  explicitly not attempt-local. The spec also supports multiple ProviderInstanceIds
  sharing one ProviderKind. That means an instance-specific request-id header
  convention cannot be represented without leaking an override across every attempt
  in the route.

  What decision is still missing or inconsistent: the spec needs to decide whether
  request-id extraction is fixed per ProviderKind, configurable per
  ProviderInstanceId, or allowed as an attempt-local override.

  Exact section references: “TransportOptions” REFACTOR2.md:465,
  “ResolvedTransportOptions” REFACTOR2.md:492, “AttemptExecutionOptions”
  REFACTOR2.md:671, “RegisteredProvider” REFACTOR2.md:1343, “ProviderDescriptor”
  REFACTOR2.md:1367, “ProviderConfig” REFACTOR2.md:1397, “PlatformConfig”
  REFACTOR2.md:1424

  6. Medium: Effective model resolution is required internally but never actually
     specified.

  Why it is a design problem: Target.model is optional, ProviderConfig.default_model
  is optional, but ResolvedProviderAttempt.model and ResponseMeta.selected_model are
  required. The routed flow says “resolve effective model” but never locks what
  happens when neither source exists.

  What decision is still missing or inconsistent: the spec needs a concrete rule for
  missing-model behavior: planning error, configuration error, or provider-driven
  implicit defaulting.

  Exact section references: “Target” REFACTOR2.md:650, “ExecutionPlan”
  REFACTOR2.md:1141, “ProviderConfig” REFACTOR2.md:1397, “Routed Toolkit Flow”
  REFACTOR2.md:2171

  Open Questions

  - Are ProviderCapabilities intended to be fixed per ProviderKind, or can a
    registered instance narrow them for self-hosted / generic endpoints?
  - When fallback stops on an executed failure, is ordered attempt history supposed
    to be attached to the terminal error the same way it is attached to success and
    all-skipped planning failures?
  - Is the direct low-level API intentionally excluding a single call that combines
    explicit model override with AttemptExecutionOptions, or is that surface just
    incomplete in the spec?

  Assumptions The Spec Appears To Rely On

  - Every transportable provider request can derive one concrete HTTP method without
    the spec needing to name the owner.
  - Request-id header naming is effectively stable per ProviderKind, except for a
    whole-call override.
  - Routed streaming fallback either never happens after first output or does not
    need a separately locked policy.
  - Every executable attempt can always resolve a model from Target.model or
    provider-instance config.
  - OpenRouterOptions.max_tokens and stream_options are not intended to redefine the
    locked task/execution contract, even though the current spec leaves that
    unstated.

