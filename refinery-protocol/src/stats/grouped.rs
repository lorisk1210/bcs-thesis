// stats/grouped.rs
// Grouped-template schema, codec, and rendering helpers.

// Third-party library imports
use anyhow::{Result, anyhow};
use serde_json::{Map, Value, json};

// Local module imports
use super::StatisticsSchema;
use super::encoding::{decode_count, decode_fixed, encode_count, encode_fixed};
use crate::errors::invalid_stats_shape;
use crate::query::QueryTemplate;

const GROUP_SLOT_STRIDE: usize = 2;
const GENDER_GROUPS: &[&str] = &["female", "male", "other", "unknown"];
const DOSE_GROUPS: &[&str] = &["high", "low", "medium"];

pub(crate) fn schema_for_query(
    template: QueryTemplate,
    params: &Value,
) -> Result<Option<StatisticsSchema>> {
    match template {
        QueryTemplate::SubgroupEffectEstimate => subgroup_schema(params).map(Some),
        QueryTemplate::DoseResponseTrend => Ok(Some(grouped_schema(
            template,
            "dose_bucket",
            DOSE_GROUPS.iter().map(|label| label.to_string()).collect(),
        ))),
        _ => Ok(None),
    }
}

pub(crate) fn encode_stats(
    template: QueryTemplate,
    slot_labels: &[String],
    stats: &Value,
) -> Option<Result<Vec<u64>>> {
    matches!(
        template,
        QueryTemplate::SubgroupEffectEstimate | QueryTemplate::DoseResponseTrend
    )
    .then(|| encode_group_stats(slot_labels, stats))
}

pub(crate) fn decode_stats(
    template: QueryTemplate,
    slot_labels: &[String],
    slots: &[u64],
) -> Option<Result<Value>> {
    match template {
        QueryTemplate::SubgroupEffectEstimate => Some(
            decode_group_stats(slot_labels, slots, "subgroup").map(|groups| json!({ "groups": groups })),
        ),
        QueryTemplate::DoseResponseTrend => Some(
            decode_group_stats(slot_labels, slots, "dose_bucket")
                .map(|groups| json!({ "groups": groups })),
        ),
        _ => None,
    }
}

pub(crate) fn render_result(template: QueryTemplate, stats: &Value) -> Option<Result<Value>> {
    let group_key = match template {
        QueryTemplate::SubgroupEffectEstimate => "subgroup",
        QueryTemplate::DoseResponseTrend => "dose_bucket",
        _ => return None,
    };

    Some(render_groups(stats, group_key).map(|groups| json!({ "groups": groups })))
}

pub(crate) fn cohort_size_from_slots(
    template: QueryTemplate,
    slot_labels: &[String],
    slots: &[u64],
) -> Option<Result<usize>> {
    matches!(
        template,
        QueryTemplate::SubgroupEffectEstimate | QueryTemplate::DoseResponseTrend
    )
    .then(|| {
        let mut total = 0usize;
        for index in (0..slot_labels.len()).step_by(GROUP_SLOT_STRIDE) {
            total = total
                .checked_add(decode_count(slots[index])? as usize)
                .ok_or_else(|| anyhow!("cohort size overflow"))?;
        }
        Ok(total)
    })
}

fn subgroup_schema(params: &Value) -> Result<StatisticsSchema> {
    let subgroup = params
        .get("subgroup")
        .and_then(Value::as_str)
        .unwrap_or("gender")
        .to_ascii_lowercase();

    if subgroup == "age_bucket" {
        let labels = age_bucket_labels(params);
        return Ok(grouped_schema(
            QueryTemplate::SubgroupEffectEstimate,
            &format!("age_bucket:{}", labels.join("|")),
            labels,
        ));
    }

    Ok(grouped_schema(
        QueryTemplate::SubgroupEffectEstimate,
        "gender",
        GENDER_GROUPS.iter().map(|label| label.to_string()).collect(),
    ))
}

fn age_bucket_labels(params: &Value) -> Vec<String> {
    let mut cutoffs: Vec<i64> = params
        .get("age_cutoffs")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_else(|| vec![40, 65]);
    cutoffs.sort();
    cutoffs.dedup();
    if cutoffs.is_empty() {
        cutoffs = vec![40, 65];
    }

    let mut labels = vec!["unknown".to_string()];
    let first = cutoffs[0];
    labels.push(format!("<{first}"));
    for window in cutoffs.windows(2) {
        labels.push(format!("[{},{})", window[0], window[1]));
    }
    let last = *cutoffs.last().unwrap_or(&65);
    labels.push(format!(">={last}"));
    labels
}

fn grouped_schema(
    template: QueryTemplate,
    schema_variant: &str,
    labels: Vec<String>,
) -> StatisticsSchema {
    StatisticsSchema {
        template,
        schema_id: format!("{}:{schema_variant}:v1", template.as_str()),
        slot_labels: build_group_slot_labels(&labels),
    }
}

fn build_group_slot_labels(labels: &[String]) -> Vec<String> {
    let mut slot_labels = Vec::with_capacity(labels.len() * GROUP_SLOT_STRIDE);
    for label in labels {
        slot_labels.push(group_n_slot_label(label));
        slot_labels.push(group_outcome_slot_label(label));
    }
    slot_labels
}

fn encode_group_stats(slot_labels: &[String], stats: &Value) -> Result<Vec<u64>> {
    let groups = stats
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_stats_shape("groups"))?;
    let mut slots = vec![0u64; slot_labels.len()];

    for group in groups {
        let label = required_group_label(group)?;
        let Some(index) = find_group_slot_index(slot_labels, label) else {
            return Err(anyhow!("unexpected group label '{label}'"));
        };
        slots[index] = encode_count(required_u64(group, "n")?)?;
        slots[index + 1] = encode_fixed(required_f64(group, "outcome_sum")?)?;
    }

    Ok(slots)
}

fn decode_group_stats(slot_labels: &[String], slots: &[u64], group_key: &str) -> Result<Value> {
    let mut groups = Vec::new();
    for index in (0..slot_labels.len()).step_by(GROUP_SLOT_STRIDE) {
        let label = group_label_from_n_slot(&slot_labels[index])?;
        let n = decode_count(slots[index])?;
        if n == 0 {
            continue;
        }

        let mut group = Map::new();
        group.insert(group_key.to_string(), Value::String(label));
        group.insert("n".to_string(), Value::from(n));
        group.insert("outcome_sum".to_string(), Value::from(decode_fixed(slots[index + 1])?));
        groups.push(Value::Object(group));
    }
    Ok(Value::Array(groups))
}

fn render_groups(stats: &Value, group_key: &str) -> Result<Value> {
    let groups = stats
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_stats_shape(group_key))?;

    let mut rendered = Vec::with_capacity(groups.len());
    for group in groups {
        let label = group
            .get(group_key)
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_stats_shape(group_key))?;
        let n = required_u64(group, "n")?;
        let outcome_sum = required_f64(group, "outcome_sum")?;
        let mut map = Map::new();
        map.insert(group_key.to_string(), Value::String(label.to_string()));
        map.insert("n".to_string(), Value::from(n));
        map.insert("mean_outcome".to_string(), json!(safe_mean(outcome_sum, n)));
        rendered.push(Value::Object(map));
    }
    Ok(Value::Array(rendered))
}

fn required_group_label(group: &Value) -> Result<&str> {
    group
        .get("subgroup")
        .or_else(|| group.get("dose_bucket"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_stats_shape("group_label"))
}

fn find_group_slot_index(slot_labels: &[String], label: &str) -> Option<usize> {
    let exact = group_n_slot_label(label);
    slot_labels.iter().position(|slot| slot == &exact).or_else(|| {
        let normalized = label.trim().to_ascii_lowercase();
        (normalized != label)
            .then(|| {
                slot_labels
                    .iter()
                    .position(|slot| slot == &group_n_slot_label(&normalized))
            })
            .flatten()
    })
}

fn group_n_slot_label(label: &str) -> String {
    format!("group:{label}:n")
}

fn group_outcome_slot_label(label: &str) -> String {
    format!("group:{label}:outcome_sum")
}

fn group_label_from_n_slot(slot_label: &str) -> Result<String> {
    slot_label
        .strip_prefix("group:")
        .and_then(|value| value.strip_suffix(":n"))
        .map(str::to_string)
        .ok_or_else(|| invalid_stats_shape("group_slot_label"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_stats_shape(key))
}

fn required_f64(value: &Value, key: &str) -> Result<f64> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| invalid_stats_shape(key))
}

fn safe_mean(sum: f64, n: u64) -> Option<f64> {
    (n > 0).then_some(sum / n as f64)
}
