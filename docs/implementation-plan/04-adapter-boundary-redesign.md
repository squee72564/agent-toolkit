# Phase 4: Adapter Boundary Redesign

## Goal

Redesign the provider boundary so adapters consume `ExecutionPlan` and are
selected by `ProviderKind`, while shared family behavior moves into family
codecs and provider-specific behavior moves into overlays.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- provider layer ownership
- new `ProviderAdapter` contract
- `ProviderDescriptor`
- `ProviderCapabilities` as adapter-exposed static metadata
- family codec and provider overlay composition
- `EncodedFamilyRequest` as the family-layer intermediate plan shape
- adapter-owned request planning artifacts
- deterministic adapter-planning rejection behavior at the adapter boundary
- adapter-owned response and error decoding orchestration
- adapter-owned stream projector creation
- adapter/runtime ownership split for `PlatformConfig` composition
- the closed `HttpRequestOptions` boundary and provider-generated dynamic
  headers

## Current Repo Anchors

- [crates/agent-providers/src/adapter.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/adapter.rs)
- [crates/agent-providers/src/request_plan.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/request_plan.rs)
- [crates/agent-providers/src/openai_family/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/openai_family/mod.rs)
- [crates/agent-providers/src/anthropic_family/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/anthropic_family/mod.rs)
- [crates/agent-providers/src/platform/openai/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/platform/openai/mod.rs)
- [crates/agent-providers/src/platform/openrouter/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/platform/openrouter/mod.rs)
- [crates/agent-providers/src/platform/anthropic/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/src/platform/anthropic/mod.rs)

## Planned Additions

- Replace `ProviderAdapter::id()` with kind- and descriptor-oriented identity.
- Replace `plan_request(Request)` with `plan_request(&ExecutionPlan)`.
- Introduce explicit composition points for family codecs and provider overlays.
- Move default endpoint path and request-id header ownership into descriptors.
- Make adapter-exposed static metadata explicit through `descriptor()` and
  `capabilities()`.
- Key descriptors by `ProviderKind` and select family codecs from
  `ProviderDescriptor.family`.
- Make `EncodedFamilyRequest` the family-codec intermediate that overlays refine
  before adapter finalization.
- Make `ProviderRequestPlan` the explicit adapter output for:
  `method`, `body`, `endpoint_path_override`, `provider_headers`,
  `HttpRequestOptions`, and `response_framing`.
- Make adapter decode orchestration explicit:
  provider-overlay success/projector override first, family fallback second;
  family error decode first, provider-overlay refinement second.
- Lock adapter inputs to resolved provider attempts from `ExecutionPlan`, not
  routing-layer `AttemptSpec`.
- Keep deterministic local request-shape validation inside `plan_request()`,
  with rejection before transport execution.
- Keep timeout resolution, retry resolution, URL construction, and auth/header
  merge ownership out of adapters.

## File-Sized Steps

1. Redefine `ProviderAdapter` around `ProviderKind` and descriptor metadata.
2. Add or expand `ProviderDescriptor` and adapter-exposed capabilities to cover
   family identity, protocol identity, default endpoint path, default
   request-id header, streaming/native-option support, and other static
   adapter-owned facts.
3. Change request planning to consume `ExecutionPlan` rather than the old
   generic request type.
4. Introduce `EncodedFamilyRequest` as the family-codec output carrying the
   pre-overlay body, warnings, method, response framing, endpoint override,
   provider headers, and `HttpRequestOptions`.
5. Split provider request planning into:
   method, request body, endpoint path override, provider headers,
   `HttpRequestOptions`, and response framing.
6. Wire codec selection through `ProviderDescriptor.family` and keep descriptor
   identity keyed by `ProviderKind`, not `ProviderInstanceId`.
7. Split overlay responsibilities so overlays may refine request body,
   protocol-level request hints, provider-generated headers, and endpoint
   override, but not runtime-owned transport controls.
8. Make `plan_request()` responsible for deterministic local validation
   failures before transport execution.
9. Split decode orchestration into:
   family decode first, then provider overlay refinement.
10. Reframe OpenRouter as an OpenAI-compatible family codec plus an OpenRouter
   overlay.
11. Add `GenericOpenAiCompatible` as a first-class provider kind where the
   descriptor/overlay split requires it.
12. Remove adapter responsibility for building runtime-owned `PlatformConfig`.
13. Make the success-decode and stream-projector override order explicit:
   provider overlay gets first override chance; family codec is the default
   fallback when the overlay returns `None`.
14. Make the adapter/runtime split explicit for dynamic headers and
    `HttpRequestOptions`: adapters produce them, runtime transports them
    without reopening metadata escape hatches.

## Locked Rules To Encode

- adapters own protocol-specific request planning artifacts
- adapters are the only layer that selects outbound HTTP method and transport
  response framing
- adapters expose descriptors keyed by `ProviderKind`, not by
  `ProviderInstanceId`
- family codecs are selected by `ProviderFamilyId` from the resolved provider
  descriptor
- family codecs produce the initial `EncodedFamilyRequest` and set
  family-default method, framing, endpoint override, and protocol hints
  consistent with the requested `ResponseMode`
- `ProviderRequestPlan.endpoint_path_override` is the only adapter-controlled
  endpoint-selection surface
- adapters emit provider-generated dynamic headers only through
  `ProviderRequestPlan.provider_headers`
- adapters own only closed protocol hints in `HttpRequestOptions`; they do not
  own timeout, retry, auth, URL, request-id extraction, or caller-header
  concerns
- family codecs consume family-scoped native options
- provider overlays consume provider-scoped native options
- provider overlays may refine request body, `HttpRequestOptions`,
  provider-generated headers, and endpoint override for concrete-provider
  behavior
- family-codec error decoding runs before provider-overlay error decoding when
  error-body preservation is enabled
- provider-overlay success decode and stream-projector creation run before
  falling back to family-codec defaults
- adapters consume resolved provider data from `ExecutionPlan`; they do not
  consume routing-layer `AttemptSpec`
- deterministic local `plan_request()` failures are planning-time rejections,
  not executed failures
- adapters expose static metadata via `ProviderDescriptor` and
  `ProviderCapabilities`; runtime consumes that metadata during planning
- runtime does not invent provider/protocol hints on behalf of adapters
- runtime, not adapters, composes `PlatformConfig` from `ProviderDescriptor`
  plus `ProviderConfig`

## Repo-Structure Guidance

- keep family codec code in `*_family` modules
- keep concrete provider overlays in provider-specific modules
- do not let adapter modules recover transport control through generic metadata
  fields
- do not let adapter helpers mutate typed timeout selection or retry policy
- keep decode orchestration centralized in adapter-facing entry points rather
  than re-embedding composition order in each provider module

## Exit Criteria

- adapter selection is by `ProviderKind`
- shared provider-family behavior is explicit and reusable
- adapters consume `ExecutionPlan`
- descriptors, not ad hoc adapter methods, own static provider metadata
- descriptors are keyed by `ProviderKind` and family codec selection is driven
  by descriptor family identity
- family codecs emit an explicit `EncodedFamilyRequest` intermediate before
  overlay refinement
- `ProviderRequestPlan` is the only adapter-produced request contract handed
  back to runtime
- adapters no longer build `PlatformConfig` directly
- adapter-exposed capabilities remain explicit and are not inferred from
  provider-instance config
- deterministic adapter-planning rejections remain explicit pre-execution
  outcomes at this boundary
- success decode, error decode, and stream-projector orchestration match the
  locked family-codec/provider-overlay ordering from `REFACTOR.md`
