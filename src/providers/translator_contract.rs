use crate::core::types::{ProviderRequest, ProviderResponse};

pub trait ProviderTranslator {
    type RequestPayload;
    type ResponsePayload;
    fn encode_request(&self, req: &ProviderRequest) -> Result<Self::RequestPayload, dyn std::Error + Send + Sync>;
    fn encode_request(&self, payload: &ResponsePayload) -> Result<Self::RequestPayload, dyn std::Error + Send + Sync>;
}
