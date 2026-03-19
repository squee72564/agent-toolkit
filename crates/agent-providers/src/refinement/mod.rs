mod anthropic;
mod generic_openai_compatible;
mod openai;
mod openrouter;
mod openrouter_stream_projector;

pub(crate) use anthropic::AnthropicOverlay;
pub(crate) use generic_openai_compatible::GenericOpenAiCompatibleOverlay;
pub(crate) use openai::OpenAiOverlay;
pub(crate) use openrouter::OpenRouterOverlay;
