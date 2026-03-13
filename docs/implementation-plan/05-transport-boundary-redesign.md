# Phase 5: Transport Boundary Redesign

## Goal

Replace metadata-driven transport control with the typed runtime-to-transport
boundary from `REFACTOR.md`.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- the locked runtime/provider/transport ownership boundary
- the long-term typed transport contract replacing metadata-driven control
- transport layer ownership
- `TransportResponseFraming`
- `TransportExecutionInput`
- `ResolvedTransportOptions`
- `HttpRequestOptions` ownership limits
- adapter-owned method and transport-framing selection
- route-wide vs attempt-local transport ownership and non-overlap rules
- typed request-id override and extra header semantics
- typed timeout ownership
- resolved intra-attempt retry ownership
- first-byte timeout remaining transport-internal
- runtime-owned URL construction
- runtime validation of framing against `ExecutionOptions.response_mode`
- transport-owned auth placement, low-level framed response handling, and SSE
  parsing/limit enforcement
- retirement of `AdapterContext` from the long-term transport boundary

## Current Repo Anchors

- [crates/agent-transport/src/http/request.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-transport/src/http/request.rs)
- [crates/agent-transport/src/http/transport.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-transport/src/http/transport.rs)
- [crates/agent-transport/src/http/headers.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-transport/src/http/headers.rs)
- [crates/agent-runtime/src/provider_runtime/transport.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_runtime/transport.rs)

## Planned Additions

- Add `ResolvedTransportOptions` in runtime/transport-facing code.
- Add or rename to a transport framing enum distinct from semantic response
  mode.
- Add a typed transport execution input carrying explicit platform, auth, URL,
  method, request body, provider headers, request options, and transport
  options.
- Keep the runtime-to-transport carrier aligned with `HttpSendRequest`, or a
  direct typed successor, rather than introducing another metadata-based
  compatibility channel.
- Lock the runtime/provider/transport ownership split into the transport-facing
  types so runtime normalizes transport inputs, adapters own request-planning
  outputs, and transport owns execution mechanics only.
- Remove dependency on `AdapterContext.metadata` for transport behavior.

## File-Sized Steps

1. Add `ResolvedTransportOptions` carrying:
   request-id extraction override, merged extra headers, resolved timeouts, and
   resolved retry policy.
2. Replace or evolve `HttpResponseMode` into transport-level framing
   terminology.
3. Redefine the runtime-to-transport request shape so it carries explicit:
   platform, auth, method, URL, body, response framing, request options,
   resolved transport options, and provider headers.
4. Lock `ProviderRequestPlan.method` and
   `ProviderRequestPlan.response_framing` as the only adapter-produced method
   and transport-framing inputs copied into the transport contract.
5. Update runtime URL construction so runtime resolves the effective endpoint
   path from `PlatformConfig` plus `endpoint_path_override`, then joins it into
   the final request URL before transport execution.
6. Move timeout fields out of `HttpRequestOptions` into
   `TransportTimeoutOverrides` and `ResolvedTransportOptions`.
7. Keep `HttpRequestOptions` only for protocol-level request/response hints.
8. Move request-id extraction override behavior out of metadata conventions into
   typed fields, with lookup precedence of:
   `ResolvedTransportOptions.request_id_header_override` then
   `PlatformConfig.request_id_header`.
9. Update transport header construction to merge:
   platform defaults, route-wide extra headers, attempt-local extra headers,
   adapter/provider headers, and auth.
10. Keep caller-owned transport headers and adapter-owned provider headers as
    separate typed layers in the transport request contract rather than
    pre-merging them into one opaque header map.
11. Resolve the effective intra-attempt retry policy in runtime and pass only
    the resolved policy through `ResolvedTransportOptions`.
12. Preserve first-byte timeout as transport-internal behavior governed by
    `stream_idle_timeout`, rather than exposing a new runtime-facing timeout
    field.
13. Keep transport responsible for auth placement, low-level framed response
    execution (`HttpJsonResponse`, `HttpBytesResponse`, `HttpSseResponse`), and
    SSE parsing/limit enforcement without provider-specific semantics.

## Locked Rules To Encode

- runtime normalizes route-wide and attempt-local typed transport inputs before
  calling transport
- provider planning is the single source of truth for outbound HTTP method and
  transport response framing
- transport receives a fully resolved URL and must not invent endpoint paths
- transport receives explicit method and response framing and must not infer
  either
- `HttpSendRequest`, or a direct typed successor, remains the runtime-to-
  transport contract
- runtime, not transport, resolves the effective intra-attempt retry policy
- transport applies only the resolved retry policy it was given
- `request_id_header_override` affects response-header lookup only
- `request_id_header_override` falls back to `PlatformConfig.request_id_header`
  when absent
- transport constructs final headers from explicit layers in locked precedence
  order
- runtime does not collapse header layers into one opaque map before transport
  execution
- adapters never mutate typed timeout selections
- first-byte timeout remains transport-internal and is governed by
  `stream_idle_timeout`
- transport retry remains intra-attempt behavior only
- transport owns auth placement, low-level framed response execution, and SSE
  parser-limit enforcement

## Repo-Structure Guidance

- keep runtime-owned transport normalization in runtime
- keep protocol hints and transport-owned fields separate in the type system
- keep the transport contract explicit even if the migration temporarily reuses
  `HttpSendRequest` as the carrier type
- keep caller-owned extra headers and adapter-owned provider headers separate
  until transport materializes the final outbound header map
- keep transport tests focused on typed boundary behavior rather than provider
  semantics

## Exit Criteria

- no runtime-to-transport behavior depends on `AdapterContext.metadata`
- transport inputs are fully typed
- runtime/provider/transport ownership is encoded directly in the request types
- runtime constructs the final request URL and resolved retry policy before
  transport execution
- transport materializes auth and header layers from explicit typed inputs
- transport still owns low-level framed responses and SSE parser-limit behavior
- timeout, retry, header, and framing ownership matches the spec
