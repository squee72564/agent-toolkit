use agent_core::types::{Request, Response};

pub trait ProtocolTranslator {
    type RequestPayload;
    type ResponsePayload;
    type Error: std::error::Error + Send + Sync + 'static;

    fn encode_request(&self, req: &Request) -> Result<Self::RequestPayload, Self::Error>;

    fn decode_request(&self, payload: &Self::ResponsePayload) -> Result<Response, Self::Error>;
}
