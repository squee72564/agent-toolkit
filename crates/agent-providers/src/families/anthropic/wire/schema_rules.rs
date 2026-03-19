use serde_json::{Map, Value, json};

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
    match serde_json::to_string(value) {
        Ok(serialized) => serialized,
        Err(_) => "null".to_string(),
    }
}

pub(super) fn extract_first_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start = None;
    let mut depth = 0_usize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, byte) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if *byte == b'\\' {
                escaped = true;
                continue;
            }
            if *byte == b'"' {
                in_string = false;
            }
            continue;
        }

        if *byte == b'"' {
            in_string = true;
            continue;
        }

        if *byte == b'{' {
            if start.is_none() {
                start = Some(index);
            }
            depth = depth.checked_add(1)?;
            continue;
        }

        if *byte == b'}' && depth > 0 {
            depth -= 1;
            if depth == 0
                && let Some(start_index) = start
            {
                return Some(text[start_index..=index].to_string());
            }
        }
    }

    None
}

pub(super) fn permissive_json_object_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": true
    })
}
