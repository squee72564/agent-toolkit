use std::collections::HashSet;

use serde_json::{Map, Value};

pub(super) fn is_strict_compatible_schema(schema: &Value) -> bool {
    let Some(obj) = schema.as_object() else {
        return false;
    };

    if obj.contains_key("anyOf") || obj.contains_key("oneOf") || obj.contains_key("allOf") {
        return false;
    }

    let is_object_schema = is_object_type(obj.get("type"));
    if !is_object_schema {
        if let Some(items) = obj.get("items") {
            return is_strict_compatible_schema(items);
        }

        return true;
    }

    match obj.get("additionalProperties") {
        Some(Value::Bool(false)) => {}
        _ => return false,
    }

    let empty_properties = Map::new();
    let properties = obj
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty_properties);

    let empty_required = Vec::new();
    let required = obj
        .get("required")
        .and_then(Value::as_array)
        .unwrap_or(&empty_required);

    if !required_keys_match_properties(required, properties) {
        return false;
    }

    properties.values().all(is_strict_compatible_schema)
}

fn is_object_type(type_value: Option<&Value>) -> bool {
    match type_value {
        Some(Value::String(value)) => value == "object",
        Some(Value::Array(values)) => values
            .iter()
            .any(|entry| matches!(entry, Value::String(value) if value == "object")),
        _ => false,
    }
}

fn required_keys_match_properties(required: &[Value], properties: &Map<String, Value>) -> bool {
    if properties.len() != required.len() {
        return false;
    }

    let mut required_keys = HashSet::with_capacity(required.len());
    for entry in required {
        let Some(key) = entry.as_str() else {
            return false;
        };
        if !required_keys.insert(key) {
            return false;
        }
    }

    properties
        .keys()
        .all(|property_key| required_keys.contains(property_key.as_str()))
}

pub(super) fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();

            let mut out = Map::new();
            for key in keys {
                if let Some(next) = map.get(&key) {
                    out.insert(key, canonicalize_json(next));
                }
            }

            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json).collect()),
        _ => value.clone(),
    }
}

pub(super) fn stable_json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}
