use serde_json::Value;

use crate::e2e::mock_server::CapturedRequest;

pub fn assert_post_path(request: &CapturedRequest, expected_path: &str) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, expected_path);
}

pub fn assert_auth_bearer(request: &CapturedRequest, token: &str) {
    let expected = format!("Bearer {token}");
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some(expected.as_str())
    );
}

pub fn assert_auth_api_key(request: &CapturedRequest, token: &str) {
    assert_eq!(
        request.headers.get("x-api-key").map(String::as_str),
        Some(token)
    );
}

pub fn assert_header(request: &CapturedRequest, name: &str, expected_value: &str) {
    let lowered = name.to_ascii_lowercase();
    assert_eq!(
        request.headers.get(&lowered).map(String::as_str),
        Some(expected_value)
    );
}

pub fn assert_json_string(body: &Value, pointer: &str, expected: &str) {
    assert_eq!(
        body.pointer(pointer).and_then(Value::as_str),
        Some(expected)
    );
}

pub fn assert_json_object_has_key(body: &Value, pointer: &str, key: &str) {
    let object = body
        .pointer(pointer)
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("expected JSON object at pointer {pointer}"));
    assert!(object.contains_key(key), "missing key '{key}' at {pointer}");
}

pub fn assert_json_array_len_at_least(body: &Value, pointer: &str, min_len: usize) {
    let array = body
        .pointer(pointer)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected JSON array at pointer {pointer}"));
    assert!(
        array.len() >= min_len,
        "expected array at {pointer} to have at least {min_len} elements, found {}",
        array.len()
    );
}
