//! `{{variable}}` substitution.

use std::collections::BTreeMap;

const MAX_DEPTH: usize = 8;

/// Replace `{{name}}` placeholders. Values may reference other variables
/// (resolved recursively, up to a small depth). Unknown names are left in
/// place so the user can see what's missing. Names starting with `$` are
/// dynamic: `$uuid`/`$guid`, `$timestamp`, `$isoTimestamp`, `$randomInt`.
pub fn interpolate(template: &str, vars: &BTreeMap<String, String>) -> String {
    interpolate_depth(template, vars, 0)
}

fn interpolate_depth(template: &str, vars: &BTreeMap<String, String>, depth: usize) -> String {
    if !template.contains("{{") {
        return template.to_string();
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after[..end].trim();
        let placeholder = &rest[start..start + 2 + end + 2];
        match resolve(name, vars) {
            Some(value) if depth < MAX_DEPTH => {
                out.push_str(&interpolate_depth(&value, vars, depth + 1))
            }
            Some(value) => out.push_str(&value),
            None => out.push_str(placeholder),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn resolve(name: &str, vars: &BTreeMap<String, String>) -> Option<String> {
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    if let Some(v) = vars.get(name) {
        return Some(v.clone());
    }
    match name {
        "$uuid" | "$guid" | "$randomUUID" => Some(uuid::Uuid::new_v4().to_string()),
        "$timestamp" => Some(chrono::Utc::now().timestamp().to_string()),
        "$timestampMs" => Some(chrono::Utc::now().timestamp_millis().to_string()),
        "$isoTimestamp" => {
            Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        }
        "$randomInt" => {
            let n = uuid::Uuid::new_v4().as_u128() % 1001;
            Some(n.to_string())
        }
        _ => None,
    }
}

/// Names referenced by `{{...}}` that can't be resolved.
pub fn unresolved(template: &str, vars: &BTreeMap<String, String>) -> Vec<String> {
    let resolved = interpolate(template, vars);
    let mut out = Vec::new();
    let mut rest = resolved.as_str();
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim().to_string();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        rest = &after[end + 2..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn substitutes_and_nests() {
        let v = vars(&[
            ("host", "api.test"),
            ("base", "https://{{host}}/v1"),
            ("id", "7"),
        ]);
        assert_eq!(
            interpolate("{{base}}/users/{{ id }}", &v),
            "https://api.test/v1/users/7"
        );
    }

    #[test]
    fn leaves_unknown_and_malformed() {
        let v = vars(&[]);
        assert_eq!(interpolate("a {{missing}} b {{", &v), "a {{missing}} b {{");
        assert_eq!(unresolved("{{a}} {{b}} {{a}}", &v), vec!["a", "b"]);
    }

    #[test]
    fn dynamic_variables() {
        let v = vars(&[]);
        assert_eq!(interpolate("{{$uuid}}", &v).len(), 36);
        assert!(interpolate("{{$timestamp}}", &v).parse::<i64>().is_ok());
        assert!(unresolved("{{$isoTimestamp}}", &v).is_empty());
    }

    #[test]
    fn self_reference_terminates() {
        let v = vars(&[("a", "{{a}}x")]);
        let out = interpolate("{{a}}", &v);
        assert!(out.ends_with('x'));
    }
}
