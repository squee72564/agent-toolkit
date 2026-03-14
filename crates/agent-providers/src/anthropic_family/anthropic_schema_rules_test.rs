use serde_json::json;

use super::schema_rules::{
    canonicalize_json, extract_first_json_object, permissive_json_object_schema, stable_json_string,
};

fn schema_rules_canonicalize_json_sorts_object_keys_recursively() {
    let input = json!({
        "z": {"b": 2, "a": 1},
        "a": [{"d": 4, "c": 3}, 5]
    });

    let canonical = canonicalize_json(&input);
    let as_string = stable_json_string(&canonical);

    assert_eq!(as_string, r#"{"a":[{"c":3,"d":4},5],"z":{"a":1,"b":2}}"#);
}

#[test]
fn schema_rules_extract_first_json_object_handles_escaped_quotes_and_nested_braces() {
    let text = r#"prefix {"outer":{"text":"value with \"quotes\" and { braces }","n":1}} suffix {"ignored":true}"#;

    let extracted = extract_first_json_object(text).expect("object should be extracted");
    assert_eq!(
        extracted,
        r#"{"outer":{"text":"value with \"quotes\" and { braces }","n":1}}"#
    );
}

#[test]
fn schema_rules_extract_first_json_object_returns_none_for_incomplete_object() {
    let text = r#"prefix {"outer":{"text":"unterminated"}"#;
    assert_eq!(extract_first_json_object(text), None);
}

#[test]
fn schema_rules_permissive_json_object_schema_shape_is_stable() {
    assert_eq!(
        permissive_json_object_schema(),
        json!({
            "type": "object",
            "additionalProperties": true
        })
    );
}
