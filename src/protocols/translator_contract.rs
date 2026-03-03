use crate::core::types::{Request, Response};

pub trait ProtocolTranslator {
    type RequestPayload;
    type ResponsePayload;
    fn encode_request(
        &self,
        req: &Request
    ) -> Result<Self::RequestPayload, Box<dyn std::error::Error + Send + Sync>>;
    fn decode_request(
        &self,
        payload: &Self::ResponsePayload
    ) -> Result<Response, Box<dyn std::error::Error + Send + Sync>>;
}
