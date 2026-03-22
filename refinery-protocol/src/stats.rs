use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::errors::invalid_stats_shape;
use crate::query::{ClipBounds, QueryResult, QueryTemplate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStatistics {
    pub template: QueryTemplate,
    pub cohort_size: usize,
    pub stats: Value,
}

pub fn aggregate_local_statistics(
    template: QueryTemplate,
    items: &[LocalStatistics],
) -> Result<LocalStatistics> {
    if items.is_empty() {
        return Err(anyhow!("cannot aggregate zero local statistics"));
    }

    let cohort_size = items.iter().map(|item| item.cohort_size).sum();
    let stats = match template {
        QueryTemplate::CohortFeasibilityCount => json!({
            "count": items.iter().map(|item| required_u64(&item.stats, "count")).sum::<Result<u64>>()?
        }),
        QueryTemplate::ComparativeEffectivenessDelta => json!({
            "n_exposed": items.iter().map(|item| required_u64(&item.stats, "n_exposed")).sum::<Result<u64>>()?,
            "n_control": items.iter().map(|item| required_u64(&item.stats, "n_control")).sum::<Result<u64>>()?,
            "outcome_sum_exposed": items.iter().map(|item| required_f64(&item.stats, "outcome_sum_exposed")).sum::<Result<f64>>()?,
            "outcome_sum_control": items.iter().map(|item| required_f64(&item.stats, "outcome_sum_control")).sum::<Result<f64>>()?
        }),
        QueryTemplate::TimeToEventProxy => json!({
            "n": items.iter().map(|item| required_u64(&item.stats, "n")).sum::<Result<u64>>()?,
            "sum_days_to_event": items.iter().map(|item| required_f64(&item.stats, "sum_days_to_event")).sum::<Result<f64>>()?,
            "max_days": items[0].stats.get("max_days").cloned().unwrap_or_else(|| Value::from(3650)),
        }),
        QueryTemplate::SubgroupEffectEstimate => json!({
            "groups": aggregate_group_sums(items, "subgroup")?
        }),
        QueryTemplate::DoseResponseTrend => json!({
            "groups": aggregate_group_sums(items, "dose_bucket")?
        }),
        QueryTemplate::AeIncidenceSignalProxy => json!({
            "n_exposed": items.iter().map(|item| required_u64(&item.stats, "n_exposed")).sum::<Result<u64>>()?,
            "n_control": items.iter().map(|item| required_u64(&item.stats, "n_control")).sum::<Result<u64>>()?,
            "ae_count_exposed": items.iter().map(|item| required_f64(&item.stats, "ae_count_exposed")).sum::<Result<f64>>()?,
            "ae_count_control": items.iter().map(|item| required_f64(&item.stats, "ae_count_control")).sum::<Result<f64>>()?
        }),
        QueryTemplate::DdiSignalProxy => json!({
            "n_combo": items.iter().map(|item| required_u64(&item.stats, "n_combo")).sum::<Result<u64>>()?,
            "n_a_only": items.iter().map(|item| required_u64(&item.stats, "n_a_only")).sum::<Result<u64>>()?,
            "ae_count_combo": items.iter().map(|item| required_f64(&item.stats, "ae_count_combo")).sum::<Result<f64>>()?,
            "ae_count_a_only": items.iter().map(|item| required_f64(&item.stats, "ae_count_a_only")).sum::<Result<f64>>()?
        }),
    };

    Ok(LocalStatistics {
        template,
        cohort_size,
        stats,
    })
}

pub fn render_query_result(
    aggregated: &LocalStatistics,
    clip: ClipBounds,
) -> Result<QueryResult> {
    let template = aggregated.template;
    let raw_result = match template {
        QueryTemplate::CohortFeasibilityCount => json!({
            "count": required_u64(&aggregated.stats, "count")?
        }),
        QueryTemplate::ComparativeEffectivenessDelta => {
            let n_exposed = required_u64(&aggregated.stats, "n_exposed")?;
            let n_control = required_u64(&aggregated.stats, "n_control")?;
            let sum_exposed = required_f64(&aggregated.stats, "outcome_sum_exposed")?;
            let sum_control = required_f64(&aggregated.stats, "outcome_sum_control")?;
            let mean_exposed = safe_mean(sum_exposed, n_exposed);
            let mean_control = safe_mean(sum_control, n_control);
            json!({
                "n_exposed": n_exposed,
                "n_control": n_control,
                "mean_outcome_exposed": mean_exposed,
                "mean_outcome_control": mean_control,
                "delta": match (mean_exposed, mean_control) {
                    (Some(exp), Some(ctrl)) => Some(exp - ctrl),
                    _ => None,
                }
            })
        }
        QueryTemplate::TimeToEventProxy => {
            let n = required_u64(&aggregated.stats, "n")?;
            let sum_days = required_f64(&aggregated.stats, "sum_days_to_event")?;
            json!({
                "n": n,
                "mean_days_to_event": safe_mean(sum_days, n)
            })
        }
        QueryTemplate::SubgroupEffectEstimate => json!({
            "groups": render_groups(&aggregated.stats, "subgroup")?
        }),
        QueryTemplate::DoseResponseTrend => json!({
            "groups": render_groups(&aggregated.stats, "dose_bucket")?
        }),
        QueryTemplate::AeIncidenceSignalProxy => {
            let n_exposed = required_u64(&aggregated.stats, "n_exposed")?;
            let n_control = required_u64(&aggregated.stats, "n_control")?;
            let ae_count_exposed = required_f64(&aggregated.stats, "ae_count_exposed")?;
            let ae_count_control = required_f64(&aggregated.stats, "ae_count_control")?;
            json!({
                "n_exposed": n_exposed,
                "n_control": n_control,
                "incidence_exposed": safe_rate(ae_count_exposed, n_exposed),
                "incidence_control": safe_rate(ae_count_control, n_control)
            })
        }
        QueryTemplate::DdiSignalProxy => {
            let n_combo = required_u64(&aggregated.stats, "n_combo")?;
            let n_a_only = required_u64(&aggregated.stats, "n_a_only")?;
            let ae_count_combo = required_f64(&aggregated.stats, "ae_count_combo")?;
            let ae_count_a_only = required_f64(&aggregated.stats, "ae_count_a_only")?;
            json!({
                "n_combo": n_combo,
                "n_a_only": n_a_only,
                "incidence_combo": safe_rate(ae_count_combo, n_combo),
                "incidence_a_only": safe_rate(ae_count_a_only, n_a_only)
            })
        }
    };

    Ok(QueryResult {
        template_name: template.as_str().to_string(),
        raw_result,
        cohort_size: aggregated.cohort_size,
        sensitivity: sensitivity_for(template, aggregated, clip),
    })
}

fn aggregate_group_sums(items: &[LocalStatistics], group_key: &str) -> Result<Value> {
    let mut combined: BTreeMap<String, (u64, f64)> = BTreeMap::new();

    for item in items {
        let groups = item
            .stats
            .get("groups")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid_stats_shape(group_key))?;
        for group in groups {
            let key = group
                .get(group_key)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid_stats_shape(group_key))?
                .to_string();
            let entry = combined.entry(key).or_insert((0, 0.0));
            entry.0 += required_u64(group, "n")?;
            entry.1 += required_f64(group, "outcome_sum")?;
        }
    }

    let rendered = combined
        .into_iter()
        .map(|(key, (n, outcome_sum))| {
            let mut map = Map::new();
            map.insert(group_key.to_string(), Value::String(key));
            map.insert("n".to_string(), Value::from(n));
            map.insert("outcome_sum".to_string(), Value::from(outcome_sum));
            Value::Object(map)
        })
        .collect::<Vec<_>>();

    Ok(Value::Array(rendered))
}

fn render_groups(stats: &Value, group_key: &str) -> Result<Value> {
    let groups = stats
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_stats_shape(group_key))?;

    let mut rendered = Vec::with_capacity(groups.len());
    for group in groups {
        let mut map = Map::new();
        let label = group
            .get(group_key)
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_stats_shape(group_key))?;
        let n = required_u64(group, "n")?;
        let outcome_sum = required_f64(group, "outcome_sum")?;
        map.insert(group_key.to_string(), Value::String(label.to_string()));
        map.insert("n".to_string(), Value::from(n));
        map.insert("mean_outcome".to_string(), json!(safe_mean(outcome_sum, n)));
        rendered.push(Value::Object(map));
    }

    Ok(Value::Array(rendered))
}

fn sensitivity_for(template: QueryTemplate, aggregated: &LocalStatistics, clip: ClipBounds) -> f64 {
    match template {
        QueryTemplate::CohortFeasibilityCount => 1.0,
        QueryTemplate::ComparativeEffectivenessDelta
        | QueryTemplate::SubgroupEffectEstimate
        | QueryTemplate::DoseResponseTrend => clipped_mean_sensitivity(clip, aggregated.cohort_size),
        QueryTemplate::TimeToEventProxy => {
            let max_days = aggregated
                .stats
                .get("max_days")
                .and_then(Value::as_f64)
                .unwrap_or(3650.0);
            max_days / aggregated.cohort_size.max(1) as f64
        }
        QueryTemplate::AeIncidenceSignalProxy | QueryTemplate::DdiSignalProxy => {
            inverse_count_sensitivity(aggregated.cohort_size)
        }
    }
}

fn clipped_mean_sensitivity(clip: ClipBounds, cohort_size: usize) -> f64 {
    (clip.max - clip.min).abs() / cohort_size.max(1) as f64
}

fn inverse_count_sensitivity(cohort_size: usize) -> f64 {
    1.0 / cohort_size.max(1) as f64
}

fn required_u64(value: &Value, key: &str) -> Result<u64> {
    value.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_stats_shape(key))
}

fn required_f64(value: &Value, key: &str) -> Result<f64> {
    value.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| invalid_stats_shape(key))
}

fn safe_mean(sum: f64, n: u64) -> Option<f64> {
    (n > 0).then_some(sum / n as f64)
}

fn safe_rate(count: f64, n: u64) -> Option<f64> {
    (n > 0).then_some(count / n as f64)
}
