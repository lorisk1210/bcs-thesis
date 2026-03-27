// stats/helpers.rs
// Shared JSON extraction, rendering, and sensitivity helpers for statistics modules.

// Third-party library imports
use anyhow::Result;
use serde_json::Value;

// Local module imports
use crate::errors::invalid_stats_shape;
use crate::query::ClipBounds;

pub(crate) fn clipped_mean_sensitivity(clip: ClipBounds, cohort_size: usize) -> f64 {
    (clip.max - clip.min).abs() / cohort_size.max(1) as f64
}

pub(crate) fn required_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_stats_shape(key))
}

pub(crate) fn required_f64(value: &Value, key: &str) -> Result<f64> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| invalid_stats_shape(key))
}

pub(crate) fn required_i64(value: &Value, key: &str, default: Option<i64>) -> Result<i64> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .or(default)
        .ok_or_else(|| invalid_stats_shape(key))
}

pub(crate) fn safe_mean(sum: f64, n: u64) -> Option<f64> {
    (n > 0).then_some(sum / n as f64)
}

pub(crate) fn safe_rate(count: f64, n: u64) -> Option<f64> {
    (n > 0).then_some(count / n as f64)
}
