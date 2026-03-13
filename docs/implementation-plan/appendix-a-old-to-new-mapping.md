# Appendix A: Old-to-New Mapping

Use this table during implementation to make sure migration work is translating
concepts into the target architecture, not carrying old coupling forward.

## Request and Execution Model

- old `Request.model_id` -> `Target.model` or `ProviderConfig.default_model`
- old `Request.stream` -> `ExecutionOptions.response_mode`
- old semantic request content -> `TaskRequest`
- old request metadata intended for provider payloads -> `TaskRequest.metadata`

## Send Options and Routing

- old `SendOptions.target` -> `Route.primary` or generated single-attempt route
- old `SendOptions.fallback_policy` -> `Route.fallback_policy`
- old `SendOptions.observer` -> `ExecutionOptions.observer`
- old `SendOptions.metadata` transport-style escape hatch -> split between
  `TaskRequest.metadata`, `ExecutionOptions.transport`, and
  `AttemptExecutionOptions.extra_headers`; do not preserve generic metadata as a
  transport control channel

## Target Identity

- old `Target.provider: ProviderId` -> `Target.instance: ProviderInstanceId`
- old provider identity overload -> split into `ProviderFamilyId`,
  `ProviderKind`, and `ProviderInstanceId`
- old provider-kind-based lookup for config and adapter -> adapter lookup uses
  `ProviderKind`, config lookup uses `ProviderInstanceId`

## Fallback

- old fallback targets on `FallbackPolicy` -> ordered `Route.primary` plus
  `Route.fallbacks`
- old retry-on-status and retry-on-transport toggles -> migration-only shims at
  most; target architecture uses `FallbackRule`s
- old `FallbackMode` -> migration-only compatibility, not target architecture

## Planner and Adapter Boundary

- old ad hoc request planning in runtime -> centralized planner
- old `ProviderAdapter::plan_request(Request)` -> `plan_request(&ExecutionPlan)`
- old adapter-owned platform config construction -> runtime-owned
  `ProviderDescriptor + ProviderConfig -> PlatformConfig`
- old unresolved route-layer input crossing provider boundary ->
  `ExecutionPlan` only

## Transport Boundary

- old metadata-driven transport control -> typed `TransportOptions`,
  `TransportTimeoutOverrides`, and `ResolvedTransportOptions`
- old `AdapterContext.metadata` header/request-id overrides -> typed transport
  fields
- old `HttpResponseMode` naming -> transport-level framing concept
- old timeout fields on `HttpRequestOptions` -> typed timeout ownership on
  execution and resolved transport option types

## Streaming

- old stream selection embedded in request -> route-wide `ResponseMode`
- old ambiguous fallback during stream handling -> explicit pre-commit fallback
  only
- old stream finish path -> terminal completion path aligned with the
  two-phase streaming contract
