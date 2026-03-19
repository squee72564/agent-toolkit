mod anthropic;
mod anthropic_stream_projector;
mod openai_compatible;
mod openai_compatible_stream_projector;

pub(crate) use anthropic::AnthropicFamilyCodec;
pub(crate) use openai_compatible::OpenAiCompatibleFamilyCodec;

#[cfg(test)]
mod tests;
