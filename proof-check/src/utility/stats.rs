use std::cmp::Ordering;

use serde_json::Value;

pub const EPSILON: f64 = 1e-12;

pub fn safe_relative_gap(released: f64, exact: f64) -> Option<f64> {
    if exact.abs() <= EPSILON {
        None
    } else {
        Some((released - exact).abs() / exact.abs())
    }
}

pub fn safe_ratio(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator.abs() <= EPSILON {
        None
    } else {
        Some(numerator / denominator)
    }
}

pub fn max_numeric_value(value: Option<&Value>) -> Option<f64> {
    let mut numbers = Vec::new();
    if let Some(value) = value {
        collect_numbers(value, &mut numbers);
    }
    max_value(&numbers)
}

fn collect_numbers(value: &Value, numbers: &mut Vec<f64>) {
    match value {
        Value::Number(number) => {
            if let Some(number) = number.as_f64().filter(|number| number.is_finite()) {
                numbers.push(number);
            }
        }
        Value::Object(map) => {
            for child in map.values() {
                collect_numbers(child, numbers);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_numbers(child, numbers);
            }
        }
        _ => {}
    }
}

pub fn min_value(values: &[f64]) -> Option<f64> {
    values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .min_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal))
}

pub fn median_value(values: &[f64]) -> Option<f64> {
    let mut values = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(values[mid])
    } else {
        Some((values[mid - 1] + values[mid]) / 2.0)
    }
}

pub fn max_value(values: &[f64]) -> Option<f64> {
    values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .max_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal))
}

pub fn format_optional_number(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.6}"))
        .unwrap_or_else(|| "n/a".to_string())
}
