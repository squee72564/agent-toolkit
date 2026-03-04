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

    let properties = obj
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let required = obj
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if properties.len() != required.len() {
        return false;
    }

    for key in properties.keys() {
        let present = required
            .iter()
            .filter_map(Value::as_str)
            .any(|required_key| required_key == key);

        if !present {
            return false;
        }
    }

    properties.values().all(is_strict_compatible_schema)
}

fn is_object_type(type_value: Option<&Value>) -> bool {
    match type_value {
        Some(Value::String(value)) => value == "object",
        Some(Value::Array(values)) => values.iter().any(|entry| entry == "object"),
        _ => false,
    }
}

pub(super) fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();

            let mut out = Map::new();
            for key in keys {
                let next = map.get(&key).expect("key collected from object must exist");
                out.insert(key, canonicalize_json(next));
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
