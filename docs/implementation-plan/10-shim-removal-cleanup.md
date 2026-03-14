# Phase 10: Shim Removal / Cleanup

## Goal

Complete the refactor by proving the new architecture by
removing any temporary migration-only code paths.

## Spec Coverage

This phase must fully cover the `REFACTOR.md` material for:

- every required coverage addition listed in `REFACTOR.md`
- migration/compatibility cleanup
- final removal of temporary shims

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

## Verification of new structs

`REFACTOR.md` specified a number of new structures and their shapes. double check to make sure they are
all used appropriately, and if they are missing note it and how it affects the implementation and creates gaps.

```bash
318:  pub struct TransportExecutionInput {
400:  pub struct TransportExecutionInput {
453:  pub struct TaskRequest {
553:  pub struct TransportOptions {
585:  pub struct TransportTimeoutOverrides {
611:  pub struct ResolvedTransportOptions {
654:  pub struct HttpRequestOptions {
720:  pub struct ExecutionOptions {
756:  pub struct NativeOptions {
845:  pub struct Target {
873:  pub struct AttemptExecutionOptions {
1054:  pub struct AttemptSpec {
1166:  pub struct AttemptRecord {
1175:  pub struct ResponseMeta {
1184:  pub struct ExecutedFailureMeta {
1193:  pub struct RoutePlanningFailure {
1249:  pub struct AttemptSkippedEvent {
1297:  pub struct FallbackMatch {
1305:  pub struct FallbackRule {
1310:  pub struct FallbackPolicy {
1430:  pub struct Route {
1473:  pub struct ResolvedProviderAttempt {
1481:  pub struct ExecutionPlan {
1523:  pub struct OpenAiCompatibleOptions {
1542:  pub struct AnthropicFamilyOptions {
1556:  pub struct OpenAiOptions {
1575:pub struct OpenRouterOptions {
1610:  pub struct AnthropicOptions {
1680:  pub struct ProviderInstanceId(String);
1690:  pub struct RegisteredProvider {
1714:  pub struct ProviderDescriptor {
1749:  pub struct ProviderConfig {
1789:  pub struct PlatformConfig {
1829:  pub struct ProviderCapabilities {
1931:  pub struct EncodedFamilyRequest {
1960:  pub struct ProviderRequestPlan {
2032:  pub struct ProviderErrorInfo {
2204:  pub struct MessageCreateInput {
```

## Current Repo Anchors

- [crates/agent-runtime/src/test/mod.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/src/test/mod.rs)
- [crates/agent-runtime/tests/observer_test.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-runtime/tests/observer_test.rs)
- [crates/agent-providers/tests/provider_contract_test.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-providers/tests/provider_contract_test.rs)
- [crates/agent-transport/tests/http_test.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent-transport/tests/http_test.rs)
- [crates/agent/tests/e2e_router_fallback_observability_test.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent/tests/e2e_router_fallback_observability_test.rs)
- [crates/agent/examples/basic_openai.rs](/Users/arod183/ProgrammingProjects/Rust/agent-toolkit/crates/agent/examples/basic_openai.rs)


## File-Sized Steps

1. Search for and remove every `REFACTOR-SHIM` and `shim` marker and delete every remaining migration-
   only compatibility path that does not belong to the target architecture.
2. Verify and check that all proposed types exist in the new target architecture for the refactor and that old
   migration compatibility and legacy types have be correctly replaced

## Exit Criteria

- every checkbox above is complete
- fixture tests remain green and preserve provider payload expectations
- no `REFACTOR-SHIM:` markers remain
- no migration-only compatibility shims from `REFACTOR.md` remain in shipped code

## Closeout Status

This cleanup slice is now closed in shipped code:

- public `AttemptMeta` has been removed from `agent-runtime` and `agent`
- executed observer/event helpers route through `AttemptRecord`-based metadata
  construction rather than a legacy per-attempt metadata surface
- `ResponseMeta`, `ExecutedFailureMeta`, and `RoutePlanningFailure` share the
  target `AttemptRecord` history shape
- no `REFACTOR-SHIM:` markers remain in shipped code
- the required workspace verification suite passes:
  - `cargo check --workspace --all-targets --locked`
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo clippy --workspace --lib --all-features -- -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic`
  - `cargo test --workspace --all-targets --all-features -- --quiet`
  - `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --document-private-items`
