• # OpenAI-family streaming direction

  ## Decision

  OpenAiResponsesStreamEvent should not become the runtime abstraction for OpenAI-family streaming.

  Reason:

  - It is only a loose deserialization envelope.
  - It does not materially simplify stream projection.
  - The current duplication problem is between OpenAI-family projectors, not between typed and untyped parsing.

  Planned direction:

  - keep it out of runtime
  - refactor OpenAI-family Responses streaming around one shared projector
  - eventually remove OpenAiResponsesStreamEvent from production code and its fixture-deserialization tests once the cleanup lands

  ## Current state

  Today:

  - OpenAiResponsesStreamEvent exists in types.rs
  - it is only used by types_test.rs
  - runtime streaming is implemented by manual JSON projection in:
      - openai_compatible_stream_projector.rs
      - openrouter_stream_projector.rs

  This means the type is effectively redundant for runtime behavior.

  ## Why OpenAiResponsesStreamEvent is the wrong runtime abstraction

  The current struct is an optional-field envelope:

  - event_type
  - response
  - item
  - part
  - output_index
  - content_index
  - item_id
  - delta
  - sequence_number

  That shape is fine for permissive fixture deserialization, but weak for projection logic:

  - event-specific invariants are not encoded
  - the projector still needs nearly the same branching and field inspection
  - most nested payloads remain untyped Values
  - it does not reduce the real behavioral complexity

  So wiring it into runtime would mostly add an extra conversion layer without removing much logic.

  ## Refactor plan for OpenAI-family streaming

  ### Scope

  This refactor should be OpenAI-family-specific.

  It should not try to unify Anthropic and other non-OpenAI streaming protocols under a deeper shared internal model.

  The cross-family abstraction is already correct:

  - streaming.rs defines ProviderStreamProjector

  That remains the only universal streaming contract.

  ### New internal structure

  Introduce a shared OpenAI-family projector module, for example:

  - crates/agent-providers/src/openai_family/responses_stream_projector.rs

  Core shape:

  pub(crate) struct ResponsesStreamProjector<C> {
      config: C,
      response_started: bool,
      completed: bool,
  }

  Config trait:

  pub(crate) trait ResponsesStreamProjectorConfig: Send + 'static {
      fn provider_kind(&self) -> ProviderKind;

      fn parse_usage(&self, value: &serde_json::Value) -> Option<Usage>;

      fn handle_done_sentinel(&self) -> Option<FinishReason> {
          None
      }

      fn validate_json_event(&self, _value: &serde_json::Value) -> Result<(), AdapterError> {
          Ok(())
      }
  }

  ### Shared event handling

  The shared projector owns projection for:

  - response.created
  - response.in_progress
  - response.output_item.added
  - response.output_text.delta
  - response.function_call_arguments.delta
  - response.output_item.done
  - response.completed
  - error

  The first refactor should continue operating on serde_json::Value to preserve current behavior and minimize risk.

  ### Provider-specific config

  OpenAI config:

  - provider_kind() -> ProviderKind::OpenAi
  - parse_usage() maps:
      - input_tokens
      - output_tokens
      - input_tokens_details.cached_tokens
      - total_tokens

  OpenRouter config:

  - provider_kind() -> ProviderKind::OpenRouter
  - parse_usage() maps:
      - prompt_tokens or input_tokens
      - completion_tokens or output_tokens
      - prompt_tokens_details.cached_tokens or input_tokens_details.cached_tokens
      - total_tokens
  - handle_done_sentinel() can map raw [DONE] behavior if needed

  This makes future OpenAI-compatible providers cheap to add:

  - add config
  - wire codec/overlay
  - avoid copying projector logic

  ## Eventual removal of OpenAiResponsesStreamEvent

  Once the shared projector refactor is in place, OpenAiResponsesStreamEvent should be removed from:

  - production code in types.rs
  - fixture tests in types_test.rs

  Why removal is appropriate:

  - it is not part of runtime behavior
  - it duplicates protocol knowledge without enforcing semantics
  - it creates the impression of a shared typed runtime model that does not actually exist

  The associated test coverage should move toward projector behavior, not passive envelope deserialization.

  ## Future typing: what to do instead

  If stronger typing becomes valuable later, replace the loose envelope with a tagged event enum designed for projection.

  Recommended shape:

  enum ResponsesStreamEvent {
      ResponseCreated { response: ResponsesBodyLite },
      ResponseInProgress { response: ResponsesBodyLite },
      ResponseOutputItemAdded { output_index: u32, item: Value },
      ResponseOutputTextDelta {
          output_index: Option<u32>,
          content_index: Option<u32>,
          item_id: Option<String>,
          delta: String,
      },
      ResponseFunctionCallArgumentsDelta {
          output_index: Option<u32>,
          item_id: Option<String>,
          delta: String,
      },
      ResponseOutputItemDone { output_index: u32, item: Value },
      ResponseCompleted { response: OpenAiResponsesBody },
      Error {
          error: Option<OpenAiErrorEnvelope>,
          message: Option<String>,
      },
      Unknown {
          event_type: Option<String>,
          raw: Value,
      },
  }

  Guidance:

  - include Unknown for forward compatibility
  - keep item as Value initially unless repeated nested handling justifies stronger typing
  - use this only after projector deduplication, not during it

  ## Multi-family, multi-provider architecture

  The library should continue to scale by protocol family, not by forcing all providers into one internal event model.

  ### Stable cross-family contract

  All providers implement:

  - ProviderStreamProjector

  That is the correct universal boundary.

  ### Family-level specialization

  Each protocol family gets its own internal streaming implementation:

  - OpenAI-family: shared Responses stream projector
  - Anthropic-family: dedicated Anthropic projector
  - future families: separate if their wire protocol differs materially

  ### Provider-level composition

  Within a family:

  - providers reuse the family projector
  - small config hooks capture provider-specific quirks

  This is the scaling model:

  - share by wire protocol family
  - not by vendor name
  - not by “all LLM providers”

  ## Resulting architectural rules

  1. ProviderStreamProjector remains the only global stream abstraction.
  2. OpenAI-family providers share one internal Responses projector.
  3. Anthropic remains separate.
  4. Future providers reuse a family projector only if they share the same wire protocol semantics.
  5. Passive envelope types that are not used by runtime projection should not remain in the codebase.

  ## Practical outcome

  Short term:

  - treat OpenAiResponsesStreamEvent as redundant
  - do not wire it into runtime

  Medium term:

  - deduplicate OpenAI-family stream projection into one shared projector over Value

  Long term:

  - if typing is still useful, add a tagged event enum with Unknown
  - remove the old loose envelope entirely

  ## Summary decision

  - OpenAiResponsesStreamEvent: remove eventually
  - OpenAI-family stream direction: one shared Responses projector
  - Typing direction: only via a future tagged enum, not the current optional-field struct
  - Multi-provider strategy: share within protocol families, not across all families
