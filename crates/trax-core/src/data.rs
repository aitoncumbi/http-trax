//! Data files for data-driven runs: one iteration per row, its values as
//! variables. CSV (a header row, RFC 4180 quoting) or a JSON array of
//! objects, as in Postman's collection runner.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::error::{Error, Result};

/// One iteration's variables.
pub type DataRow = BTreeMap<String, String>;

/// Read a data file; `.json` (or content starting with `[`) is JSON,
/// anything else CSV.
pub fn load_data(path: &Path) -> Result<Vec<DataRow>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Invalid(format!("can't read {}: {e}", path.display())))?;
    parse_data(&text, &path.to_string_lossy())
}

/// Parse data file contents. `name` is the file name, used to pick the
/// format and in error messages.
pub fn parse_data(text: &str, name: &str) -> Result<Vec<DataRow>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let json = name.to_ascii_lowercase().ends_with(".json") || text.trim_start().starts_with('[');
    let rows = if json {
        parse_json(text)
    } else {
        parse_csv(text)
    };
    rows.map_err(|e| Error::Invalid(format!("{name}: {e}")))
}

fn parse_json(text: &str) -> std::result::Result<Vec<DataRow>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Value::Array(items) = value else {
        return Err("expected a JSON array of objects".into());
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| match item {
            Value::Object(map) => Ok(map
                .into_iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::String(s) => s,
                        Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (k, v)
                })
                .collect()),
            _ => Err(format!("row {} is not an object", i + 1)),
        })
        .collect()
}

fn parse_csv(text: &str) -> std::result::Result<Vec<DataRow>, String> {
    let mut records = csv_records(text)?.into_iter();
    let Some(header) = records.next() else {
        return Ok(Vec::new());
    };
    let header: Vec<String> = header.into_iter().map(|h| h.trim().to_string()).collect();
    if let Some(dup) = header
        .iter()
        .enumerate()
        .find(|(i, h)| !h.is_empty() && header[..*i].contains(h))
    {
        return Err(format!("column {:?} appears twice", dup.1));
    }
    Ok(records
        // A blank line is not a row.
        .filter(|r| !(r.len() == 1 && r[0].is_empty()))
        .map(|record| {
            header
                .iter()
                .zip(record)
                .filter(|(h, _)| !h.is_empty())
                .map(|(h, v)| (h.clone(), v))
                .collect()
        })
        .collect())
}

/// Split CSV into records: commas separate fields, double quotes wrap
/// fields that contain commas, quotes (doubled) or line breaks.
fn csv_records(text: &str) -> std::result::Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut line = 1;
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => {
                if c == '\n' {
                    line += 1;
                }
                field.push(c);
            }
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n') => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                line += 1;
            }
            (false, c) => field.push(c),
        }
    }
    if quoted {
        return Err(format!("unclosed quote (from line {line})"));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pairs: &[(&str, &str)]) -> DataRow {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parses_csv() {
        let text = "\u{feff}user, qty ,note\r\nada,1,\"likes \"\"tea\"\", and cake\"\r\n\r\ngrace,2,\"two\nlines\"\nlinus,3\n";
        assert_eq!(
            parse_data(text, "rows.csv").unwrap(),
            [
                row(&[
                    ("user", "ada"),
                    ("qty", "1"),
                    ("note", "likes \"tea\", and cake")
                ]),
                row(&[("user", "grace"), ("qty", "2"), ("note", "two\nlines")]),
                // A short row leaves the missing columns unset.
                row(&[("user", "linus"), ("qty", "3")]),
            ]
        );
        assert!(parse_data("a,a\n1,2", "x.csv")
            .unwrap_err()
            .to_string()
            .contains("\"a\" appears twice"));
        assert!(parse_data("a\n\"open", "x.csv")
            .unwrap_err()
            .to_string()
            .contains("unclosed quote"));
        assert!(parse_data("", "empty.csv").unwrap().is_empty());
    }

    #[test]
    fn parses_json() {
        let text = r#"[{"user": "ada", "qty": 1, "admin": true, "tags": ["a"], "none": null}]"#;
        assert_eq!(
            parse_data(text, "rows.json").unwrap(),
            [row(&[
                ("user", "ada"),
                ("qty", "1"),
                ("admin", "true"),
                ("tags", "[\"a\"]"),
                ("none", "")
            ])]
        );
        // Detected by content too.
        assert_eq!(
            parse_data(" []", "data.txt").unwrap(),
            Vec::<DataRow>::new()
        );
        assert!(parse_data(r#"{"a": 1}"#, "x.json")
            .unwrap_err()
            .to_string()
            .contains("array of objects"));
        assert!(parse_data("[1]", "x.json")
            .unwrap_err()
            .to_string()
            .contains("row 1 is not an object"));
    }
}
