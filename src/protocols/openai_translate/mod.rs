use serde_json::{Map, Value, json};

use crate::core::types::{
    Request, Response, ResponseFormat
};

use crate::protocols::translator_contract::ProtocolTranslator;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenAiEncodedRequest {
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenAiDecodeEnvelope {
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAiErrorEnvelope {
    pub message: String,
    pub code: Option<String>,
    pub error_type: Option<String>,
    pub param: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OpenAiTranslator;

impl ProtocolTranslator for OpenAiTranslator {
    type RequestPayload = OpenAiEncodedRequest;
    type ResponsePayload = OpenAiDecodeEnvelope;

    fn encode_request(
        &self,
        req: &Request
    ) -> Result<Self::RequestPayload, Box<dyn std::error::Error + Send + Sync>>
    {
        encode_openai_request(req)
    }

    fn decode_request(
        &self,
        payload: &Self::ResponsePayload
    ) -> Result<Response, Box<dyn std::error::Error + Send + Sync>>
    {
        decode_openai_request(payload)
    }
}

pub(crate) fn encode_openai_request(

) -> Result<OpenAiEncodedRequest, Box<dyn std::error::Error + Send + Sync>> {

    let text_format = map_response_format(req)?;
    let tools_choice = map_tool_choice(req)?;
    let tools = map_tools(req)?;
    let input = map_messages(req)?;

    let mut body = Map::new();

    body.insert(
        "model".to_string(),
        Value::String(req.model_id.clone()),
    );

    body.insert("store".to_string(), Value::Bool(false));
    body.insert("input".to_string(), Value::Array(input));
    body.insert("text".to_string(), json!({ "format": text_format }));

    if !tools.is_empty() {
        body.insert("tools".to_string(), Value::Array(tools));
    }

    body.insert("tool_choice".to_string(), tool_choice);

    if let Some(temperature) = req.temperature {
        body.insert("temperature".to_string(), json!(temperature));
    }

    if let Some(max_output_tokens) = req.max_output_tokens {
        body.insert("max_output_tokens".to_string(), json!(max_output_tokens));
    }

    if !req.metadata.is_empty() {
        body.insert("metadata".to_string(), json!(req.metadata));
    }

    Ok(OpenAiEncodedRequest {
        body: Value::Object(body),
    })
}

fn map_response_format(req: &Request) -> Value {
    match &req.response_format {
        ResponseFormat::Text => json!({ "type": "text"}),
        ResponseFormat::JsonObject => {
           json!({"type": "json_object"}) 
        },
        ResponseFormat::JsonSchema { name, schema } => {
            json!({
                "type": "json_schema",
                "name": name,
                "schema": schema,
                "strict": true,
            })
        },
    }
}
