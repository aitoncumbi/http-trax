//! Lenient argument parsing: models are generous in what they send, so accept
//! the common shapes (maps or key/value lists, JSON bodies as objects, ...).

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Map, Value};
use trax_core::{Body, Contract, DataRow, KeyValue, Variable};

pub fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    opt_str(args, key).ok_or_else(|| format!("missing required argument `{key}`"))
}

pub fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

pub fn opt_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

pub fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// `{"a": 1}` or `[{"key": "a", "value": 1}]` → map.
pub fn string_map(v: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    match v {
        Some(Value::Object(map)) => {
            for (k, v) in map {
                out.insert(k.clone(), scalar_to_string(v));
            }
        }
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(k) = item.get("key").and_then(Value::as_str) {
                    out.insert(
                        k.to_string(),
                        item.get("value").map(scalar_to_string).unwrap_or_default(),
                    );
                }
            }
        }
        _ => {}
    }
    out
}

/// Headers/params: a map or a list of `{key, value, enabled?, description?}`.
pub fn key_values(v: &Value) -> Result<Vec<KeyValue>, String> {
    match v {
        Value::Object(map) => Ok(map
            .iter()
            .map(|(k, v)| KeyValue::new(k.clone(), scalar_to_string(v)))
            .collect()),
        Value::Array(items) => items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                if let Some(v) = item.get("value").cloned() {
                    item["value"] = Value::String(scalar_to_string(&v));
                }
                serde_json::from_value(item).map_err(|e| format!("invalid key/value entry: {e}"))
            })
            .collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err("expected an object or a list of {key, value}".into()),
    }
}

/// Variables: a map or a list of `{key, value, secret?, enabled?}`.
pub fn variables(v: &Value) -> Result<Vec<Variable>, String> {
    match v {
        Value::Object(map) => Ok(map
            .iter()
            .map(|(k, v)| Variable::new(k.clone(), scalar_to_string(v)))
            .collect()),
        Value::Array(items) => items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                if let Some(v) = item.get("value").cloned() {
                    item["value"] = Value::String(scalar_to_string(&v));
                }
                serde_json::from_value(item).map_err(|e| format!("invalid variable: {e}"))
            })
            .collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err("expected an object or a list of {key, value}".into()),
    }
}

/// A body given as a string, a JSON value, or a `{mode, ...}` object.
pub fn body(v: &Value) -> Result<Body, String> {
    match v {
        Value::Null => Ok(Body::None),
        Value::String(s) if s.trim().is_empty() => Ok(Body::None),
        Value::String(s) => {
            if serde_json::from_str::<Value>(s).is_ok_and(|v| v.is_object() || v.is_array()) {
                Ok(Body::Json { text: s.clone() })
            } else {
                Ok(Body::Text {
                    text: s.clone(),
                    content_type: None,
                })
            }
        }
        Value::Object(map) if map.contains_key("mode") => {
            let mut map = map.clone();
            // Allow `{"mode": "json", "json": {...}}` and `{"mode":"json","text":{...}}`.
            if map.get("mode").and_then(Value::as_str) == Some("json") {
                for key in ["json", "text", "content"] {
                    if let Some(v) = map.get(key).cloned() {
                        if !v.is_string() {
                            map.insert("text".into(), Value::String(pretty(&v)));
                        } else if key != "text" {
                            map.insert("text".into(), v);
                        }
                    }
                }
            }
            if map.get("mode").and_then(Value::as_str) == Some("form-urlencoded")
                || map.get("mode").and_then(Value::as_str) == Some("multipart")
            {
                if let Some(Value::Object(fields)) = map.get("fields").cloned() {
                    let list: Vec<Value> = fields
                        .iter()
                        .map(|(k, v)| json!({ "key": k, "value": scalar_to_string(v) }))
                        .collect();
                    map.insert("fields".into(), Value::Array(list));
                }
            }
            serde_json::from_value(Value::Object(map)).map_err(|e| format!("invalid body: {e}"))
        }
        other => Ok(Body::Json {
            text: pretty(other),
        }),
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// A contract: `{"openapi": path}`, `{"schema": path}`, `{"off": true}`,
/// `false` (off), or `null` (none, so it inherits).
pub fn contract(v: &Value) -> Result<Option<Contract>, String> {
    match v {
        Value::Null => Ok(None),
        Value::Bool(false) => Ok(Some(Contract {
            off: true,
            ..Default::default()
        })),
        Value::Object(_) => serde_json::from_value(v.clone())
            .map(|c: Contract| {
                (c.off || !c.openapi.is_empty() || !c.schema.is_empty()).then_some(c)
            })
            .map_err(|e| format!("invalid contract: {e}")),
        _ => Err(
            "invalid contract: expected {\"openapi\": path}, {\"schema\": path}, {\"off\": true} or null"
                .into(),
        ),
    }
}

/// Rows for a data-driven run: an array of objects, or the path of a CSV or
/// JSON file relative to the workspace root.
pub fn data(root: &Path, v: Option<&Value>) -> Result<Vec<DataRow>, String> {
    let rows = match v {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::String(path)) => trax_core::load_data(&root.join(path)),
        Some(rows @ Value::Array(_)) => trax_core::parse_data(&rows.to_string(), "data.json"),
        Some(_) => return Err("`data` must be an array of objects or a file path".into()),
    }
    .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        return Err("`data` has no rows".into());
    }
    Ok(rows)
}

/// Accept snake_case aliases inside `scripts`.
pub fn scripts(v: &Value) -> Value {
    let mut out = Map::new();
    if let Value::Object(map) = v {
        for (k, val) in map {
            let key = match k.as_str() {
                "pre_request" | "pre" | "prerequest" => "preRequest",
                "post_response" | "post" | "tests" | "test" => "postResponse",
                other => other,
            };
            out.insert(key.to_string(), val.clone());
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies() {
        assert_eq!(
            body(&json!({"a": 1})).unwrap(),
            Body::Json {
                text: "{\n  \"a\": 1\n}".into()
            }
        );
        assert!(matches!(
            body(&json!("{\"a\":1}")).unwrap(),
            Body::Json { .. }
        ));
        assert!(matches!(body(&json!("plain")).unwrap(), Body::Text { .. }));
        assert_eq!(
            body(&json!({"mode": "json", "json": {"b": true}})).unwrap(),
            Body::Json {
                text: "{\n  \"b\": true\n}".into()
            }
        );
        match body(&json!({"mode": "form-urlencoded", "fields": {"a": 1}})).unwrap() {
            Body::FormUrlencoded { fields } => assert_eq!(fields[0].value, "1"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn maps_and_lists() {
        assert_eq!(
            key_values(&json!({"Accept": "x"})).unwrap()[0].key,
            "Accept"
        );
        assert_eq!(
            key_values(&json!([{"key": "n", "value": 5}])).unwrap()[0].value,
            "5"
        );
        assert_eq!(string_map(Some(&json!({"id": 7})))["id"], "7");
        assert!(variables(&json!([{"key": "k", "value": "v", "secret": true}])).unwrap()[0].secret);
        assert_eq!(scripts(&json!({"tests": "x"}))["postResponse"], "x");
    }
}
