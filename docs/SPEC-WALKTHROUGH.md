# SPEC Walkthrough: Current Workspace Architecture

This document is a guided reading of the current `agent-toolkit` workspace. It is not a second source of truth. Use it together with:

- generated rustdoc from `cargo doc` for the current public API surface
- the source files under `crates/*` for implementation details

The workspace is organized around four stable boundaries:

- semantic request content in `agent_core`
- routing and runtime orchestration in `agent_runtime`
- provider translation in `agent_providers`
- HTTP execution in `agent_transport`

The user-facing entry point is the `agent_toolkit` facade crate in `crates/agent`, which reexports the workspace APIs.

## 1. Workspace Map

Read the crates in this order if you want the fastest path from public API to implementation:

- `crates/agent/src/lib.rs` - `agent_toolkit` facade and reexports
- `crates/agent-core/src/lib.rs` - shared request, response, identity, planning, and streaming types
- `crates/agent-runtime/src/lib.rs` - clients, routing, attempt metadata, observers, and runtime orchestration
- `crates/agent-providers/src/lib.rs` - provider adapters, family codecs, and provider refinements
- `crates/agent-transport/src/lib.rs` - HTTP/SSE transport and request/response helpers
- `crates/agent-tools/src/lib.rs` - tool builder, registry, and tool execution runtime

## 2. Public API Shape

The facade crate is intentionally shallow. It reexports the workspace surface so most consumers can stay on `agent_toolkit` while still having access to the lower-level crates when they need them.

The important top-level split is:

- `agent_toolkit::core` and `agent_toolkit::request` for shared types
- `agent_toolkit::runtime` for routing and provider execution
- `agent_toolkit::protocols` for adapter internals
- `agent_toolkit::transport` for HTTP transport primitives
- `agent_toolkit::tools` for tool runtime support

Direct clients and routed clients are built on top of the same underlying flow:

- direct use: `openai()`, `anthropic()`, `openrouter()`
- routed use: `AgentToolkit`

If you need the shape of the exposed surface, check:

- `crates/agent/src/lib.rs`
- `crates/agent-core/src/types/mod.rs`
- `crates/agent-runtime/src/lib.rs`
- `crates/agent-providers/src/lib.rs`
- `crates/agent-transport/src/lib.rs`

## 3. Core Request Model

The stable runtime boundary is still the three-way split between semantic input, routing, and execution behavior.

```text
TaskRequest + Route + ExecutionOptions
```

- `TaskRequest` owns semantic request content only.
- `Route` owns ordered target selection and routing policy.
- `ExecutionOptions` owns route-wide execution behavior such as response mode, observer hooks, and transport overrides.

The main data flow looks like this:

```text
MessageCreateInput
  -> TaskRequest
  -> Route / AttemptSpec / Target
  -> ExecutionOptions / AttemptExecutionOptions
  -> ExecutionPlan
  -> ProviderRequestPlan
  -> TransportExecutionInput
  -> Response or RuntimeError
```

`AttemptExecutionOptions` is the attempt-local escape hatch for timeout overrides, native options, and extra headers. Do not use it for route-wide behavior.

## 4. Identity And Configuration

Provider identity is split into three different concerns:

- `ProviderFamilyId` selects the shared wire family.
- `ProviderKind` selects the concrete adapter and refinement behavior.
- `ProviderInstanceId` selects one registered runtime destination.

That split matters because routes target instances, not kinds. A route can still share adapter and codec behavior across multiple instances that happen to use the same family or provider kind.

Configuration follows the same separation:

- `ProviderDescriptor` is adapter-owned static metadata.
- `ProviderConfig` is runtime-owned per-instance configuration.
- `PlatformConfig` is the resolved transport-facing result of those two inputs.

If you are debugging provider resolution, start in:

- `crates/agent-core/src/types/identity.rs`
- `crates/agent-core/src/types/platform.rs`

## 5. Provider Layering

`agent-providers` is the translation layer between the core model and provider-specific wire formats.

The runtime talks to a `ProviderAdapter`. The adapter composes:

- a family codec for shared protocol behavior
- a provider refinement for provider-specific behavior

The flow is:

1. family codec turns `TaskRequest` into `EncodedFamilyRequest`
2. provider refinement mutates that into `ProviderRequestPlan`
3. runtime turns that into `TransportExecutionInput`

The family codec owns shared encode/decode behavior. The refinement layer owns provider-specific mutations, error refinement, and optional decode or stream-projector overrides.

For the full layering walk-through, use `docs/provider-layering.md`.

## 6. Routing, Fallback, And Planning

Routing is ordered and explicit.

- `Route` owns the primary attempt and any fallbacks.
- `FallbackPolicy` evaluates executed failures only.
- `PlanningRejectionPolicy` decides what to do with attempts rejected before execution begins.

That separation is important:

- planning rejections are not fallback candidates
- executed failures are
- skipped attempts still appear in history

The runtime records ordered attempt history in `AttemptRecord`, and it uses `RoutePlanningFailure` when routing stops before any executed attempt succeeds or fails.

Success and failure metadata are separate from planning history:

- `ResponseMeta` describes a successful routed call
- `ExecutedFailureMeta` describes a failed executed attempt
- `RoutePlanningFailure` describes a route that never reached execution

If all candidates are exhausted before execution, `RoutePlanningFailureReason` tells you whether nothing was compatible or every attempt was rejected during planning.

## 7. Streaming And Observability

Streaming is a two-phase contract.

- `ResponseMode::Streaming` opens a canonical event stream.
- The first canonical event commits the attempt.
- Fallback is only allowed before that commit point.

The caller-visible streaming path uses:

- `MessageResponseStream` for canonical envelopes
- `MessageTextStream` for text deltas
- `StreamCompletion` for terminal stream finalization

Observers get structured lifecycle events for:

- request start and end
- attempt start, success, and failure
- skipped attempts during planning

Skipped attempts do not imply provider execution started. They are recorded and observed separately from executed failures.

## 8. Transport Boundary

`agent-transport` owns HTTP execution, SSE parsing, retries inside one attempt, and timeout enforcement.

The typed runtime-to-transport contract is `TransportExecutionInput`. The runtime resolves method, URL, body, framing, auth, headers, and timeout policy before transport execution begins.

Transport does not decide:

- fallback
- provider-specific decode
- routing order
- provider semantics

It only executes the typed request it receives.

Header layering and request-id extraction are also runtime-owned decisions. If you need to debug those, start in:

- `crates/agent-transport/src/http/request.rs`
- `crates/agent-runtime/src/provider_runtime/transport.rs`

## 9. Tools

`agent-tools` is separate from model tool-calls.

Use `agent_toolkit::tools` or `crates/agent-tools/src/lib.rs` when you want:

- `ToolBuilder` for constructing tools
- `ToolRegistry` for registering them
- `ToolRuntime` for validating and executing tool calls

Use `agent_core::types::tool` when you only need the data model for tool definitions and tool results.

## 10. Practical Reading Order

If you are new to the repository, read in this order:

1. `crates/agent/src/lib.rs`
2. `crates/agent-core/src/lib.rs`
3. `crates/agent-runtime/src/lib.rs`
4. `docs/provider-layering.md`
5. `crates/agent-providers/src/lib.rs`
6. `crates/agent-transport/src/lib.rs`
7. `crates/agent-tools/src/lib.rs`

That order matches the flow from user-facing facade, to shared types, to runtime orchestration, to provider translation, to transport execution, to tool support.
