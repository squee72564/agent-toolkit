use reqwest::header::{CONTENT_TYPE, HeaderMap};

use crate::http::request::{HeaderConfig, HttpResponseHead};

pub(crate) fn build_response_head(
    response: &reqwest::Response,
    header_config: &HeaderConfig,
) -> HttpResponseHead {
    HttpResponseHead {
        status: response.status(),
        headers: response.headers().clone(),
        request_id: extract_request_id(response.headers(), header_config),
    }
}

pub(crate) fn content_type_matches(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.split(';').next())
        .map(str::trim)
        .is_some_and(|content_type| content_type.eq_ignore_ascii_case(expected))
}

fn extract_request_id(headers: &HeaderMap, header_config: &HeaderConfig) -> Option<String> {
    headers
        .get(&header_config.request_id_header)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}
