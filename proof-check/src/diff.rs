use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::DiffEntry;

pub(crate) fn diff_payloads(left: &Value, right: &Value) -> Vec<DiffEntry> {
    let mut diffs = Vec::new();
    compare_json("$", left, right, &mut diffs);
    diffs
}

fn compare_json(path: &str, left: &Value, right: &Value, diffs: &mut Vec<DiffEntry>) {
    match (left, right) {
        (Value::Null, Value::Null) => {}
        (Value::Bool(a), Value::Bool(b)) if a == b => {}
        (Value::String(a), Value::String(b)) if a == b => {}
        (Value::Number(a), Value::Number(b)) => {
            if !numbers_match(a, b) {
                diffs.push(DiffEntry {
                    path: path.to_string(),
                    left: Value::Number(a.clone()),
                    right: Value::Number(b.clone()),
                });
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                diffs.push(DiffEntry {
                    path: format!("{path}.length"),
                    left: json!(a.len()),
                    right: json!(b.len()),
                });
                return;
            }
            for (index, (left_item, right_item)) in a.iter().zip(b.iter()).enumerate() {
                compare_json(&format!("{path}[{index}]"), left_item, right_item, diffs);
            }
        }
        (Value::Object(a), Value::Object(b)) => compare_objects(path, a, b, diffs),
        _ => diffs.push(DiffEntry {
            path: path.to_string(),
            left: left.clone(),
            right: right.clone(),
        }),
    }
}

fn compare_objects(
    path: &str,
    left: &Map<String, Value>,
    right: &Map<String, Value>,
    diffs: &mut Vec<DiffEntry>,
) {
    let keys = left
        .keys()
        .chain(right.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for key in keys {
        let child_path = format!("{path}.{key}");
        match (left.get(&key), right.get(&key)) {
            (Some(left_value), Some(right_value)) => {
                compare_json(&child_path, left_value, right_value, diffs);
            }
            (Some(left_value), None) => diffs.push(DiffEntry {
                path: child_path,
                left: left_value.clone(),
                right: Value::Null,
            }),
            (None, Some(right_value)) => diffs.push(DiffEntry {
                path: child_path,
                left: Value::Null,
                right: right_value.clone(),
            }),
            (None, None) => {}
        }
    }
}

fn numbers_match(left: &serde_json::Number, right: &serde_json::Number) -> bool {
    match (
        left.as_i64().or_else(|| left.as_u64().map(|value| value as i64)),
        right.as_i64().or_else(|| right.as_u64().map(|value| value as i64)),
    ) {
        (Some(left_int), Some(right_int)) => left_int == right_int,
        _ => {
            let left_f64 = left.as_f64().unwrap_or(f64::NAN);
            let right_f64 = right.as_f64().unwrap_or(f64::NAN);
            let abs_diff = (left_f64 - right_f64).abs();
            let rel_diff = abs_diff / left_f64.abs().max(right_f64.abs()).max(1.0);
            abs_diff <= 1e-9 || rel_diff <= 1e-9
        }
    }
}
