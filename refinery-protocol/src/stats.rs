// src/stats.rs
// Canonical sufficient-statistics encoding and aggregation shared by every federation mode.

// Third-party library imports
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

// Local module imports
use crate::errors::invalid_stats_shape;
use crate::query::{ClipBounds, QueryResult, QueryTemplate};

const FIXED_POINT_SCALE: f64 = 1_000_000_000.0;
const MAX_SAFE_MAGNITUDE: i64 = i64::MAX / 4;
const GENDER_GROUPS: &[&str] = &["female", "male", "other", "unknown"];
const DOSE_GROUPS: &[&str] = &["high", "low", "medium"];

// Canonical slot-vector schema for one query request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatisticsSchema {
    pub template: QueryTemplate,
    pub schema_id: String,
    pub slot_labels: Vec<String>,
}

// Canonical sufficient statistics computed by one node for one query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalStatistics {
    pub template: QueryTemplate,
    pub schema_id: String,
    pub slot_labels: Vec<String>,
    pub slots: Vec<u64>,
    pub cohort_size: usize,
}

impl LocalStatistics {
    // Builds canonical statistics from template-specific JSON stats.
    pub fn from_stats_value(
        template: QueryTemplate,
        params: &Value,
        stats: Value,
        cohort_size: usize,
    ) -> Result<Self> {
        let schema = schema_for_query(template, params)?;
        let slots = encode_stats_value(template, &schema.slot_labels, &stats)?;
        Ok(Self {
            template,
            schema_id: schema.schema_id,
            slot_labels: schema.slot_labels,
            slots,
            cohort_size,
        })
    }

    // Reconstructs the JSON statistics view used by result rendering.
    pub fn to_stats_value(&self) -> Result<Value> {
        decode_stats_value(self.template, &self.slot_labels, &self.slots)
    }

    // Encodes the canonical slots as little-endian bytes for SMPC transport.
    pub fn encode_slot_bytes(&self) -> Vec<u8> {
        encode_slot_bytes(&self.slots)
    }

    // Reconstructs canonical statistics from encoded slot bytes.
    pub fn from_slot_bytes(
        template: QueryTemplate,
        schema_id: String,
        slot_labels: Vec<String>,
        slot_bytes: &[u8],
        cohort_size: usize,
    ) -> Result<Self> {
        Ok(Self {
            template,
            schema_id,
            slot_labels,
            slots: decode_slot_bytes(slot_bytes)?,
            cohort_size,
        })
    }
}

// Derives the canonical slot schema for one validated query request.
pub fn schema_for_query(template: QueryTemplate, params: &Value) -> Result<StatisticsSchema> {
    let (schema_id, slot_labels) = match template {
        QueryTemplate::CohortFeasibilityCount => (
            format!("{}:v1", template.as_str()),
            vec!["count".to_string()],
        ),
        QueryTemplate::ComparativeEffectivenessDelta => (
            format!("{}:v1", template.as_str()),
            vec![
                "n_exposed".to_string(),
                "n_control".to_string(),
                "outcome_sum_exposed".to_string(),
                "outcome_sum_control".to_string(),
            ],
        ),
        QueryTemplate::TimeToEventProxy => (
            format!("{}:v1", template.as_str()),
            vec![
                "n".to_string(),
                "sum_days_to_event".to_string(),
                "max_days".to_string(),
            ],
        ),
        QueryTemplate::SubgroupEffectEstimate => subgroup_schema(params)?,
        QueryTemplate::DoseResponseTrend => (
            format!("{}:dose_bucket:v1", template.as_str()),
            build_group_slot_labels(DOSE_GROUPS),
        ),
        QueryTemplate::AeIncidenceSignalProxy => (
            format!("{}:v1", template.as_str()),
            vec![
                "n_exposed".to_string(),
                "n_control".to_string(),
                "ae_count_exposed".to_string(),
                "ae_count_control".to_string(),
            ],
        ),
        QueryTemplate::DdiSignalProxy => (
            format!("{}:v1", template.as_str()),
            vec![
                "n_combo".to_string(),
                "n_a_only".to_string(),
                "ae_count_combo".to_string(),
                "ae_count_a_only".to_string(),
            ],
        ),
    };

    Ok(StatisticsSchema {
        template,
        schema_id,
        slot_labels,
    })
}

// Aggregates per-node statistics into one canonical aggregate.
pub fn aggregate_local_statistics(
    template: QueryTemplate,
    items: &[LocalStatistics],
) -> Result<LocalStatistics> {
    if items.is_empty() {
        return Err(anyhow!("cannot aggregate zero local statistics"));
    }

    let first = &items[0];
    if first.template != template {
        return Err(anyhow!(
            "template mismatch: expected {}, received {}",
            template,
            first.template
        ));
    }

    let mut slots = vec![0u64; first.slots.len()];
    let mut cohort_size = 0usize;
    for item in items {
        if item.template != template {
            return Err(anyhow!(
                "template mismatch: expected {}, received {}",
                template,
                item.template
            ));
        }
        if item.schema_id != first.schema_id || item.slot_labels != first.slot_labels {
            return Err(anyhow!("statistics schema mismatch for {}", template.as_str()));
        }
        if item.slots.len() != slots.len() {
            return Err(anyhow!("slot vector length mismatch for {}", template.as_str()));
        }
        for (index, slot) in item.slots.iter().enumerate() {
            slots[index] = slots[index].wrapping_add(*slot);
        }
        cohort_size = cohort_size
            .checked_add(item.cohort_size)
            .ok_or_else(|| anyhow!("cohort size overflow"))?;
    }

    Ok(LocalStatistics {
        template,
        schema_id: first.schema_id.clone(),
        slot_labels: first.slot_labels.clone(),
        slots,
        cohort_size,
    })
}

// Aggregates already-encoded vectors, deriving the cohort size from the final slots.
pub fn aggregate_slot_vectors(
    template: QueryTemplate,
    schema_id: &str,
    slot_labels: &[String],
    slot_vectors: &[Vec<u64>],
) -> Result<LocalStatistics> {
    if slot_vectors.is_empty() {
        return Err(anyhow!("cannot aggregate zero slot vectors"));
    }

    let expected_len = slot_labels.len();
    let mut slots = vec![0u64; expected_len];
    for vector in slot_vectors {
        if vector.len() != expected_len {
            return Err(anyhow!("slot vector length mismatch for {}", template.as_str()));
        }
        for (index, slot) in vector.iter().enumerate() {
            slots[index] = slots[index].wrapping_add(*slot);
        }
    }

    let cohort_size = cohort_size_from_slots(template, slot_labels, &slots)?;
    Ok(LocalStatistics {
        template,
        schema_id: schema_id.to_string(),
        slot_labels: slot_labels.to_vec(),
        slots,
        cohort_size,
    })
}

// Renders a final query result from aggregated sufficient statistics.
pub fn render_query_result(aggregated: &LocalStatistics, clip: ClipBounds) -> Result<QueryResult> {
    let stats = aggregated.to_stats_value()?;
    let template = aggregated.template;
    let raw_result = match template {
        QueryTemplate::CohortFeasibilityCount => json!({
            "count": required_u64(&stats, "count")?
        }),
        QueryTemplate::ComparativeEffectivenessDelta => {
            let n_exposed = required_u64(&stats, "n_exposed")?;
            let n_control = required_u64(&stats, "n_control")?;
            let sum_exposed = required_f64(&stats, "outcome_sum_exposed")?;
            let sum_control = required_f64(&stats, "outcome_sum_control")?;
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
            let n = required_u64(&stats, "n")?;
            let sum_days = required_f64(&stats, "sum_days_to_event")?;
            json!({
                "n": n,
                "mean_days_to_event": safe_mean(sum_days, n)
            })
        }
        QueryTemplate::SubgroupEffectEstimate => json!({
            "groups": render_groups(&stats, "subgroup")?
        }),
        QueryTemplate::DoseResponseTrend => json!({
            "groups": render_groups(&stats, "dose_bucket")?
        }),
        QueryTemplate::AeIncidenceSignalProxy => {
            let n_exposed = required_u64(&stats, "n_exposed")?;
            let n_control = required_u64(&stats, "n_control")?;
            let ae_count_exposed = required_f64(&stats, "ae_count_exposed")?;
            let ae_count_control = required_f64(&stats, "ae_count_control")?;
            json!({
                "n_exposed": n_exposed,
                "n_control": n_control,
                "incidence_exposed": safe_rate(ae_count_exposed, n_exposed),
                "incidence_control": safe_rate(ae_count_control, n_control)
            })
        }
        QueryTemplate::DdiSignalProxy => {
            let n_combo = required_u64(&stats, "n_combo")?;
            let n_a_only = required_u64(&stats, "n_a_only")?;
            let ae_count_combo = required_f64(&stats, "ae_count_combo")?;
            let ae_count_a_only = required_f64(&stats, "ae_count_a_only")?;
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

// Encodes slot values into little-endian bytes.
pub fn encode_slot_bytes(slots: &[u64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(slots.len() * 8);
    for slot in slots {
        bytes.extend_from_slice(&slot.to_le_bytes());
    }
    bytes
}

// Decodes little-endian slot bytes into a slot vector.
pub fn decode_slot_bytes(bytes: &[u8]) -> Result<Vec<u64>> {
    if !bytes.len().is_multiple_of(8) {
        return Err(anyhow!("slot bytes length must be divisible by 8"));
    }
    let mut slots = Vec::with_capacity(bytes.len() / 8);
    for chunk in bytes.chunks_exact(8) {
        let array: [u8; 8] = chunk
            .try_into()
            .map_err(|_| anyhow!("invalid slot byte chunk"))?;
        slots.push(u64::from_le_bytes(array));
    }
    Ok(slots)
}

fn subgroup_schema(params: &Value) -> Result<(String, Vec<String>)> {
    let subgroup = params
        .get("subgroup")
        .and_then(Value::as_str)
        .unwrap_or("gender")
        .to_ascii_lowercase();

    if subgroup == "age_bucket" {
        let labels = age_bucket_labels(params);
        let schema_id = format!(
            "{}:age_bucket:{}:v1",
            QueryTemplate::SubgroupEffectEstimate.as_str(),
            labels.join("|")
        );
        Ok((schema_id, build_group_slot_labels(&labels)))
    } else {
        Ok((
            format!(
                "{}:gender:v1",
                QueryTemplate::SubgroupEffectEstimate.as_str()
            ),
            build_group_slot_labels(GENDER_GROUPS),
        ))
    }
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

fn build_group_slot_labels(groups: &[impl AsRef<str>]) -> Vec<String> {
    let mut labels = Vec::with_capacity(groups.len() * 2);
    for group in groups {
        let group = group.as_ref();
        labels.push(format!("group:{group}:n"));
        labels.push(format!("group:{group}:outcome_sum"));
    }
    labels
}

fn encode_stats_value(
    template: QueryTemplate,
    slot_labels: &[String],
    stats: &Value,
) -> Result<Vec<u64>> {
    match template {
        QueryTemplate::CohortFeasibilityCount => Ok(vec![encode_count(required_u64(stats, "count")?)?]),
        QueryTemplate::ComparativeEffectivenessDelta => Ok(vec![
            encode_count(required_u64(stats, "n_exposed")?)?,
            encode_count(required_u64(stats, "n_control")?)?,
            encode_fixed(required_f64(stats, "outcome_sum_exposed")?)?,
            encode_fixed(required_f64(stats, "outcome_sum_control")?)?,
        ]),
        QueryTemplate::TimeToEventProxy => Ok(vec![
            encode_count(required_u64(stats, "n")?)?,
            encode_fixed(required_f64(stats, "sum_days_to_event")?)?,
            encode_signed(
                stats.get("max_days")
                    .and_then(Value::as_i64)
                    .unwrap_or(3650),
            )?,
        ]),
        QueryTemplate::SubgroupEffectEstimate | QueryTemplate::DoseResponseTrend => {
            encode_group_stats(slot_labels, stats)
        }
        QueryTemplate::AeIncidenceSignalProxy => Ok(vec![
            encode_count(required_u64(stats, "n_exposed")?)?,
            encode_count(required_u64(stats, "n_control")?)?,
            encode_fixed(required_f64(stats, "ae_count_exposed")?)?,
            encode_fixed(required_f64(stats, "ae_count_control")?)?,
        ]),
        QueryTemplate::DdiSignalProxy => Ok(vec![
            encode_count(required_u64(stats, "n_combo")?)?,
            encode_count(required_u64(stats, "n_a_only")?)?,
            encode_fixed(required_f64(stats, "ae_count_combo")?)?,
            encode_fixed(required_f64(stats, "ae_count_a_only")?)?,
        ]),
    }
}

fn decode_stats_value(
    template: QueryTemplate,
    slot_labels: &[String],
    slots: &[u64],
) -> Result<Value> {
    if slot_labels.len() != slots.len() {
        return Err(anyhow!("slot label count does not match slot vector length"));
    }

    match template {
        QueryTemplate::CohortFeasibilityCount => Ok(json!({
            "count": decode_count(slots[0])?
        })),
        QueryTemplate::ComparativeEffectivenessDelta => Ok(json!({
            "n_exposed": decode_count(slots[0])?,
            "n_control": decode_count(slots[1])?,
            "outcome_sum_exposed": decode_fixed(slots[2])?,
            "outcome_sum_control": decode_fixed(slots[3])?,
        })),
        QueryTemplate::TimeToEventProxy => Ok(json!({
            "n": decode_count(slots[0])?,
            "sum_days_to_event": decode_fixed(slots[1])?,
            "max_days": decode_signed(slots[2])?,
        })),
        QueryTemplate::SubgroupEffectEstimate => Ok(json!({
            "groups": decode_group_stats(slot_labels, slots, "subgroup")?
        })),
        QueryTemplate::DoseResponseTrend => Ok(json!({
            "groups": decode_group_stats(slot_labels, slots, "dose_bucket")?
        })),
        QueryTemplate::AeIncidenceSignalProxy => Ok(json!({
            "n_exposed": decode_count(slots[0])?,
            "n_control": decode_count(slots[1])?,
            "ae_count_exposed": decode_fixed(slots[2])?,
            "ae_count_control": decode_fixed(slots[3])?,
        })),
        QueryTemplate::DdiSignalProxy => Ok(json!({
            "n_combo": decode_count(slots[0])?,
            "n_a_only": decode_count(slots[1])?,
            "ae_count_combo": decode_fixed(slots[2])?,
            "ae_count_a_only": decode_fixed(slots[3])?,
        })),
    }
}

fn encode_group_stats(slot_labels: &[String], stats: &Value) -> Result<Vec<u64>> {
    let groups = stats
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_stats_shape("groups"))?;
    let mut values = vec![0u64; slot_labels.len()];

    for group in groups {
        let label = group
            .get("subgroup")
            .or_else(|| group.get("dose_bucket"))
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_stats_shape("group_label"))?;
        let label = canonical_group_label(label);
        if let Some(index) = slot_labels
            .iter()
            .position(|slot| slot == &format!("group:{label}:n"))
        {
            values[index] = encode_count(required_u64(group, "n")?)?;
            values[index + 1] = encode_fixed(required_f64(group, "outcome_sum")?)?;
        } else if label != "unknown" {
            return Err(anyhow!("unexpected group label '{label}'"));
        }
    }

    Ok(values)
}

fn decode_group_stats(slot_labels: &[String], slots: &[u64], group_key: &str) -> Result<Value> {
    let mut groups = Vec::new();
    for index in (0..slot_labels.len()).step_by(2) {
        let label_slot = &slot_labels[index];
        let label = label_slot
            .strip_prefix("group:")
            .and_then(|value| value.strip_suffix(":n"))
            .ok_or_else(|| invalid_stats_shape("group_slot_label"))?;
        let n = decode_count(slots[index])?;
        let outcome_sum = decode_fixed(slots[index + 1])?;
        if n == 0 {
            continue;
        }

        let mut group = Map::new();
        group.insert(group_key.to_string(), Value::String(label.to_string()));
        group.insert("n".to_string(), Value::from(n));
        group.insert("outcome_sum".to_string(), Value::from(outcome_sum));
        groups.push(Value::Object(group));
    }
    Ok(Value::Array(groups))
}

fn cohort_size_from_slots(
    template: QueryTemplate,
    slot_labels: &[String],
    slots: &[u64],
) -> Result<usize> {
    match template {
        QueryTemplate::CohortFeasibilityCount => Ok(decode_count(slots[0])? as usize),
        QueryTemplate::ComparativeEffectivenessDelta
        | QueryTemplate::AeIncidenceSignalProxy => {
            let left = decode_count(slots[0])? as usize;
            let right = decode_count(slots[1])? as usize;
            left.checked_add(right)
                .ok_or_else(|| anyhow!("cohort size overflow"))
        }
        QueryTemplate::TimeToEventProxy => Ok(decode_count(slots[0])? as usize),
        QueryTemplate::SubgroupEffectEstimate | QueryTemplate::DoseResponseTrend => {
            let mut total = 0usize;
            for index in (0..slot_labels.len()).step_by(2) {
                total = total
                    .checked_add(decode_count(slots[index])? as usize)
                    .ok_or_else(|| anyhow!("cohort size overflow"))?;
            }
            Ok(total)
        }
        QueryTemplate::DdiSignalProxy => {
            let combo = decode_count(slots[0])? as usize;
            let a_only = decode_count(slots[1])? as usize;
            combo
                .checked_add(a_only)
                .ok_or_else(|| anyhow!("cohort size overflow"))
        }
    }
}

fn sensitivity_for(template: QueryTemplate, aggregated: &LocalStatistics, clip: ClipBounds) -> f64 {
    match template {
        QueryTemplate::CohortFeasibilityCount => 1.0,
        QueryTemplate::ComparativeEffectivenessDelta
        | QueryTemplate::SubgroupEffectEstimate
        | QueryTemplate::DoseResponseTrend => clipped_mean_sensitivity(clip, aggregated.cohort_size),
        QueryTemplate::TimeToEventProxy => {
            let max_days = aggregated
                .to_stats_value()
                .ok()
                .and_then(|stats| stats.get("max_days").and_then(Value::as_f64))
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

fn encode_count(value: u64) -> Result<u64> {
    let value = i64::try_from(value).map_err(|_| anyhow!("count exceeds supported range"))?;
    encode_signed(value)
}

fn decode_count(slot: u64) -> Result<u64> {
    let value = decode_signed(slot)?;
    if value < 0 {
        return Err(anyhow!("decoded count is negative"));
    }
    Ok(value as u64)
}

fn encode_fixed(value: f64) -> Result<u64> {
    if !value.is_finite() {
        return Err(anyhow!("fixed-point value must be finite"));
    }
    let scaled = (value * FIXED_POINT_SCALE).round();
    if scaled.abs() > MAX_SAFE_MAGNITUDE as f64 {
        return Err(anyhow!("fixed-point value exceeds supported range"));
    }
    encode_signed(scaled as i64)
}

fn decode_fixed(slot: u64) -> Result<f64> {
    Ok(decode_signed(slot)? as f64 / FIXED_POINT_SCALE)
}

fn encode_signed(value: i64) -> Result<u64> {
    if value.abs() > MAX_SAFE_MAGNITUDE {
        return Err(anyhow!("signed value exceeds supported range"));
    }
    Ok(value as u64)
}

fn decode_signed(slot: u64) -> Result<i64> {
    let value = slot as i64;
    if value.abs() > MAX_SAFE_MAGNITUDE {
        return Err(anyhow!("decoded value exceeds supported range"));
    }
    Ok(value)
}

fn canonical_group_label(label: &str) -> String {
    let normalized = label.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "female" | "male" | "other" | "unknown" | "low" | "medium" | "high" => normalized,
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subgroup_gender_schema_is_stable() {
        let schema = schema_for_query(QueryTemplate::SubgroupEffectEstimate, &json!({}))
            .expect("schema should build");
        assert_eq!(schema.schema_id, "subgroup_effect_estimate:gender:v1");
        assert_eq!(
            schema.slot_labels,
            vec![
                "group:female:n",
                "group:female:outcome_sum",
                "group:male:n",
                "group:male:outcome_sum",
                "group:other:n",
                "group:other:outcome_sum",
                "group:unknown:n",
                "group:unknown:outcome_sum",
            ]
        );
    }

    #[test]
    fn slot_bytes_round_trip() {
        let slots = vec![1u64, u64::MAX, 44u64];
        let encoded = encode_slot_bytes(&slots);
        let decoded = decode_slot_bytes(&encoded).expect("decode should work");
        assert_eq!(decoded, slots);
    }

    #[test]
    fn local_statistics_round_trip_preserves_rendered_values() {
        let local = LocalStatistics::from_stats_value(
            QueryTemplate::ComparativeEffectivenessDelta,
            &json!({}),
            json!({
                "n_exposed": 10,
                "n_control": 12,
                "outcome_sum_exposed": 50.25,
                "outcome_sum_control": 30.5
            }),
            22,
        )
        .expect("local stats should encode");

        let decoded = local.to_stats_value().expect("stats should decode");
        assert_eq!(decoded["n_exposed"], json!(10));
        assert_eq!(decoded["n_control"], json!(12));
        assert_eq!(decoded["outcome_sum_exposed"], json!(50.25));
        assert_eq!(decoded["outcome_sum_control"], json!(30.5));
    }

    #[test]
    fn canonical_round_trip_supports_all_templates() {
        let cases = vec![
            (
                QueryTemplate::CohortFeasibilityCount,
                json!({}),
                json!({"count": 12}),
                12usize,
            ),
            (
                QueryTemplate::ComparativeEffectivenessDelta,
                json!({}),
                json!({
                    "n_exposed": 3,
                    "n_control": 5,
                    "outcome_sum_exposed": 10.75,
                    "outcome_sum_control": 14.25
                }),
                8usize,
            ),
            (
                QueryTemplate::TimeToEventProxy,
                json!({"max_days": 90}),
                json!({
                    "n": 4,
                    "sum_days_to_event": 120.0,
                    "max_days": 90
                }),
                4usize,
            ),
            (
                QueryTemplate::SubgroupEffectEstimate,
                json!({"subgroup": "gender"}),
                json!({
                    "groups": [
                        {"subgroup": "female", "n": 2, "outcome_sum": 5.5},
                        {"subgroup": "male", "n": 1, "outcome_sum": 4.0}
                    ]
                }),
                3usize,
            ),
            (
                QueryTemplate::DoseResponseTrend,
                json!({}),
                json!({
                    "groups": [
                        {"dose_bucket": "low", "n": 2, "outcome_sum": 6.0},
                        {"dose_bucket": "high", "n": 1, "outcome_sum": 5.0}
                    ]
                }),
                3usize,
            ),
            (
                QueryTemplate::AeIncidenceSignalProxy,
                json!({}),
                json!({
                    "n_exposed": 5,
                    "n_control": 7,
                    "ae_count_exposed": 2.0,
                    "ae_count_control": 1.0
                }),
                12usize,
            ),
            (
                QueryTemplate::DdiSignalProxy,
                json!({}),
                json!({
                    "n_combo": 4,
                    "n_a_only": 6,
                    "ae_count_combo": 1.0,
                    "ae_count_a_only": 2.0
                }),
                10usize,
            ),
        ];

        for (template, params, stats, cohort_size) in cases {
            let local = LocalStatistics::from_stats_value(
                template,
                &params,
                stats.clone(),
                cohort_size,
            )
            .expect("local statistics should encode");
            let decoded = local.to_stats_value().expect("local statistics should decode");
            match template {
                QueryTemplate::SubgroupEffectEstimate | QueryTemplate::DoseResponseTrend => {
                    let mut expected_groups = stats["groups"]
                        .as_array()
                        .expect("groups should be an array")
                        .clone();
                    let mut decoded_groups = decoded["groups"]
                        .as_array()
                        .expect("groups should be an array")
                        .clone();
                    expected_groups.sort_by(|left, right| left.to_string().cmp(&right.to_string()));
                    decoded_groups.sort_by(|left, right| left.to_string().cmp(&right.to_string()));
                    assert_eq!(decoded_groups, expected_groups);
                }
                _ => assert_eq!(decoded, stats),
            }
        }
    }
}
