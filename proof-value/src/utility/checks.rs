use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::batch_models::{UtilityCheckKind, UtilityCheckResult, UtilityCheckStatus};

#[derive(Debug, Clone, Copy)]
pub enum ExtremeKind {
    Max,
    Min,
}

pub fn utility_check(
    name: &str,
    kind: UtilityCheckKind,
    status: UtilityCheckStatus,
    detail: String,
) -> UtilityCheckResult {
    UtilityCheckResult {
        name: name.to_string(),
        kind,
        status,
        detail,
    }
}

pub fn sign_stability_check(
    name: &str,
    raw_value: f64,
    released_value: f64,
    near_zero_threshold: f64,
) -> UtilityCheckResult {
    if raw_value.abs() <= near_zero_threshold {
        return utility_check(
            name,
            UtilityCheckKind::Hard,
            UtilityCheckStatus::Skipped,
            format!(
                "Exact raw value {raw_value:.6} is within near-zero threshold {near_zero_threshold:.6}."
            ),
        );
    }

    utility_check(
        name,
        UtilityCheckKind::Hard,
        if raw_value.signum() == released_value.signum() {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "raw_value={raw_value:.6}, released_value={released_value:.6}, near_zero_threshold={near_zero_threshold:.6}"
        ),
    )
}

pub fn dose_order_check(
    raw_means: &BTreeMap<String, f64>,
    released_means: &BTreeMap<String, f64>,
    tie_margin: f64,
) -> UtilityCheckResult {
    let required_buckets = ["low", "medium", "high"];
    if !required_buckets
        .iter()
        .all(|bucket| raw_means.contains_key(*bucket) && released_means.contains_key(*bucket))
    {
        return utility_check(
            "dose_bucket_ordering",
            UtilityCheckKind::Hard,
            UtilityCheckStatus::Skipped,
            "One or more of low/medium/high is missing.".to_string(),
        );
    }

    for (left, right) in [("low", "medium"), ("medium", "high")] {
        let raw_diff = raw_means[right] - raw_means[left];
        if raw_diff.abs() <= tie_margin {
            return utility_check(
                "dose_bucket_ordering",
                UtilityCheckKind::Hard,
                UtilityCheckStatus::Skipped,
                format!(
                    "Exact raw adjacent buckets {left}/{right} are tied within {tie_margin:.6}."
                ),
            );
        }
    }

    for (left, right) in [("low", "medium"), ("medium", "high"), ("low", "high")] {
        let raw_diff = raw_means[right] - raw_means[left];
        let released_diff = released_means[right] - released_means[left];
        if released_diff.abs() <= tie_margin {
            return utility_check(
                "dose_bucket_ordering",
                UtilityCheckKind::Hard,
                UtilityCheckStatus::Failed,
                format!(
                    "Released buckets {left}/{right} are tied within {tie_margin:.6}, so the ordering is not preserved."
                ),
            );
        }
        if raw_diff.signum() != released_diff.signum() {
            return utility_check(
                "dose_bucket_ordering",
                UtilityCheckKind::Hard,
                UtilityCheckStatus::Failed,
                format!(
                    "Pairwise ordering differs for {left}/{right}: raw_diff={raw_diff:.6}, released_diff={released_diff:.6}."
                ),
            );
        }
    }

    utility_check(
        "dose_bucket_ordering",
        UtilityCheckKind::Hard,
        UtilityCheckStatus::Passed,
        "All pairwise bucket orderings match exact raw.".to_string(),
    )
}

pub fn extreme_stability_check(
    name: &str,
    raw_values: &BTreeMap<String, f64>,
    released_values: &BTreeMap<String, f64>,
    kind: ExtremeKind,
    tie_margin: f64,
) -> UtilityCheckResult {
    let raw_candidates = extreme_candidates(raw_values, kind, tie_margin);
    if raw_candidates.is_empty() {
        return utility_check(
            name,
            UtilityCheckKind::Hard,
            UtilityCheckStatus::Skipped,
            "No comparable groups were available.".to_string(),
        );
    }
    if raw_candidates.len() > 1 {
        return utility_check(
            name,
            UtilityCheckKind::Hard,
            UtilityCheckStatus::Skipped,
            format!("Exact raw groups are tied within {tie_margin:.6}: {raw_candidates:?}"),
        );
    }

    let released_candidates = extreme_candidates(released_values, kind, tie_margin);
    let raw_label = raw_candidates.iter().next().cloned().unwrap_or_default();
    if released_candidates.contains(&raw_label) {
        utility_check(
            name,
            UtilityCheckKind::Hard,
            UtilityCheckStatus::Passed,
            format!("Released extreme candidates include exact raw leader {raw_label}."),
        )
    } else {
        utility_check(
            name,
            UtilityCheckKind::Hard,
            UtilityCheckStatus::Failed,
            format!(
                "Exact raw leader {raw_label} is absent from released extreme candidates {released_candidates:?}."
            ),
        )
    }
}

fn extreme_candidates(
    values: &BTreeMap<String, f64>,
    kind: ExtremeKind,
    tie_margin: f64,
) -> BTreeSet<String> {
    if values.is_empty() {
        return BTreeSet::new();
    }
    let target = match kind {
        ExtremeKind::Max => values.values().copied().fold(f64::NEG_INFINITY, f64::max),
        ExtremeKind::Min => values.values().copied().fold(f64::INFINITY, f64::min),
    };

    values
        .iter()
        .filter_map(|(key, value)| {
            if (*value - target).abs() <= tie_margin {
                Some(key.clone())
            } else {
                None
            }
        })
        .collect()
}

pub fn grouped_metric_map(
    payload: &Value,
    label_key: &str,
    metric_key: &str,
) -> Result<BTreeMap<String, f64>> {
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("expected groups array"))?;
    let mut values = BTreeMap::new();

    for group in groups {
        let label = group
            .get(label_key)
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing group label '{label_key}'"))?;
        let value = group
            .get(metric_key)
            .ok_or_else(|| anyhow!("missing group metric '{metric_key}'"))?;
        if let Some(number) = value.as_f64() {
            values.insert(label.to_string(), number);
        }
    }

    Ok(values)
}
