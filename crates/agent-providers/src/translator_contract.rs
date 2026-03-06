use agent_core::types::{Request, Response};

/// Defines the protocol translation boundary between core request/response types
/// and provider-specific wire payloads.
///
/// Implementations should:
/// - avoid panics for recoverable failures,
/// - return typed errors that preserve useful source context,
/// - keep encode/decode behavior deterministic for the same inputs.
pub trait ProtocolTranslator {
    /// Provider-specific encoded request payload produced from a [`Request`].
    type RequestPayload;
    /// Provider-specific response envelope consumed to produce a [`Response`].
    type ResponsePayload;
    /// Translator error type for encode/decode failures.
    ///
    /// This must be thread-safe and `'static` so callers can propagate it
    /// through shared runtime boundaries.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Encodes a core [`Request`] into a provider-specific request payload.
    fn encode_request(&self, req: Request) -> Result<Self::RequestPayload, Self::Error>;

    /// Decodes a provider response payload into a core [`Response`].
    ///
    /// The method name is retained for API compatibility.
    fn decode_request(&self, payload: Self::ResponsePayload) -> Result<Response, Self::Error>;
}
