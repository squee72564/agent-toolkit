//! OpenRouter translation implementation built on the OpenAI-style responses
//! API shape plus OpenRouter-specific request overrides.

pub(crate) mod request;
pub(crate) mod response;
pub(crate) mod stream;

#[cfg(test)]
mod test;

#[cfg(test)]
mod decoded_fixtures_test;
#[cfg(test)]
mod request_test;
#[cfg(test)]
mod response_test;
#[cfg(test)]
mod stream_test;
