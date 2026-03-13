# Phase 7: Streaming Commit and Finalization

## Goal

Implement the streaming-specific execution contract from `REFACTOR.md`, with
runtime-owned commit semantics and terminal finalization behavior.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- `ResponseMode::Streaming` as a route-wide mode
- the streaming API as a two-phase public contract: canonical events followed by
  one terminal completion outcome
- framing compatibility between response mode and adapter-produced transport
  framing
- fallback eligibility before the first canonical stream event
- commit point at first canonical event emission
- runtime-owned pre-commit vs post-commit streaming failure classification
- non-commit conditions for SSE open, raw SSE receipt, and projector creation
- no fallback after commit
- terminal finalization semantics on success and failure
- no partial/live `ResponseMeta` on incremental stream events

## Current Repo Anchors

- [crates/agent-runtime/src/provider_stream_runtime.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_stream_runtime.rs)
- [crates/agent-runtime/src/provider_stream_runtime/finalize.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/provider_stream_runtime/finalize.rs)
- [crates/agent-runtime/src/message_response_stream/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/message_response_stream/mod.rs)
- [crates/agent-runtime/src/direct_streaming_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/direct_streaming_api.rs)
- [crates/agent-runtime/src/routed_streaming_api.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/routed_streaming_api.rs)

## Planned Additions

- Add runtime state for fallback-eligible streaming vs committed streaming.
- Ensure streaming attempts are planned and opened against the new transport
  framing boundary.
- Align finalize paths with terminal response/error semantics.

## File-Sized Steps

1. Add runtime state that tracks whether the first canonical event has been
   emitted to the caller.
2. Update stream open logic so opening SSE or receiving raw frames does not mark
   the attempt committed.
3. Update event delivery so commit happens only on first canonical event
   emission.
4. Update error handling so runtime, not transport or provider streaming code,
   classifies streaming failures relative to the commit point.
5. Update error handling so pre-commit setup/projector/framing/EOF/finalization
   failures can still trigger fallback when allowed by route policy.
6. Update error handling so post-commit framing/projector/termination/finalize
   failures never trigger fallback and are surfaced on the active
   stream/finalization path.
7. Update finalization so terminal success yields a completed canonical
   `Response` with normal `ResponseMeta`.
8. Update finalization so terminal executed failure yields normalized
   `RuntimeError` plus `ExecutedFailureMeta`, including for committed-stream
   failures after the first canonical event.

## Locked Rules To Encode

- non-streaming mode must not internally open SSE and finalize it
- streaming mode is an additional capability, not the baseline provider
  contract
- runtime alone determines whether a streaming attempt is still fallback-eligible
  or already committed
- opening SSE, receiving raw SSE frames, or creating the provider stream
  projector must not commit the attempt
- stream events do not carry partial/live route attempt metadata
- finalize is part of the public streaming contract

## Repo-Structure Guidance

- keep stream delivery state separate from planner and route cursor state
- keep commit-point checks near the canonical event emission path
- keep transport SSE mechanics separate from provider-specific projector logic

## Exit Criteria

- fallback behavior matches the streaming commit rule exactly
- streaming APIs produce terminal outcomes aligned with the spec
- streaming failure classification matches the pre-commit/post-commit rules from
  `REFACTOR.md`
- no incremental event leaks final/partial metadata that the spec forbids
