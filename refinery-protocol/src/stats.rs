// src/stats.rs
// Canonical sufficient-statistics encoding and aggregation shared by every federation mode.

mod encoding;
mod grouped;
mod helpers;
mod scalar;

// Third-party library imports
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Local module imports
use crate::query::{ClipBounds, QueryResult, QueryTemplate};
use crate::slot_vector;

pub use crate::slot_vector::{decode_slot_bytes, encode_slot_bytes};

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
    if let Some(schema) = scalar::schema_for_query(template) {
        return Ok(schema);
    }
    if let Some(schema) = grouped::schema_for_query(template, params)? {
        return Ok(schema);
    }

    Err(anyhow!(
        "no statistics schema configured for {}",
        template.as_str()
    ))
}

// Aggregates per-node statistics into one canonical aggregate.
pub fn aggregate_local_statistics(
    template: QueryTemplate,
    items: &[LocalStatistics],
) -> Result<LocalStatistics> {
    let first = items
        .first()
        .ok_or_else(|| anyhow!("cannot aggregate zero local statistics"))?;
    validate_aggregate_items(template, &first.schema_id, &first.slot_labels, items)?;

    let mut slots = slot_vector::sum_slot_slices(
        first.slot_labels.len(),
        items.iter().map(|item| item.slots.as_slice()),
    )
    .map_err(|_| anyhow!("slot vector length mismatch for {}", template.as_str()))?;
    if let Some(result) = scalar::normalize_aggregated_slots(template, &mut slots, items.len()) {
        result?;
    }

    Ok(LocalStatistics {
        template,
        schema_id: first.schema_id.clone(),
        slot_labels: first.slot_labels.clone(),
        slots,
        cohort_size: items
            .iter()
            .try_fold(0usize, |total, item| total.checked_add(item.cohort_size))
            .ok_or_else(|| anyhow!("cohort size overflow"))?,
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

    let mut slots = slot_vector::sum_slot_slices(
        slot_labels.len(),
        slot_vectors.iter().map(|vector| vector.as_slice()),
    )
    .map_err(|_| anyhow!("slot vector length mismatch for {}", template.as_str()))?;
    if let Some(result) =
        scalar::normalize_aggregated_slots(template, &mut slots, slot_vectors.len())
    {
        result?;
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
    let raw_result = if let Some(rendered) = scalar::render_result(aggregated.template, &stats) {
        rendered?
    } else if let Some(rendered) = grouped::render_result(aggregated.template, &stats) {
        rendered?
    } else {
        return Err(anyhow!(
            "no renderer configured for {}",
            aggregated.template.as_str()
        ));
    };

    Ok(QueryResult {
        template_name: aggregated.template.as_str().to_string(),
        raw_result,
        cohort_size: aggregated.cohort_size,
        sensitivity: sensitivity_for(aggregated, clip),
        dp_release_stats: Some(stats),
        clip_bounds: Some(clip),
    })
}

fn validate_aggregate_items(
    template: QueryTemplate,
    schema_id: &str,
    slot_labels: &[String],
    items: &[LocalStatistics],
) -> Result<()> {
    for item in items {
        if item.template != template {
            return Err(anyhow!(
                "template mismatch: expected {}, received {}",
                template,
                item.template
            ));
        }
        if item.schema_id != schema_id || item.slot_labels != slot_labels {
            return Err(anyhow!(
                "statistics schema mismatch for {}",
                template.as_str()
            ));
        }
    }
    Ok(())
}

fn encode_stats_value(
    template: QueryTemplate,
    slot_labels: &[String],
    stats: &Value,
) -> Result<Vec<u64>> {
    if let Some(slots) = scalar::encode_stats(template, stats) {
        return slots;
    }
    if let Some(slots) = grouped::encode_stats(template, slot_labels, stats) {
        return slots;
    }

    Err(anyhow!(
        "no statistics encoder configured for {}",
        template.as_str()
    ))
}

fn decode_stats_value(
    template: QueryTemplate,
    slot_labels: &[String],
    slots: &[u64],
) -> Result<Value> {
    validate_slot_layout(template, slot_labels, slots.len())?;

    if let Some(stats) = scalar::decode_stats(template, slots) {
        return stats;
    }
    if let Some(stats) = grouped::decode_stats(template, slot_labels, slots) {
        return stats;
    }

    Err(anyhow!(
        "no statistics decoder configured for {}",
        template.as_str()
    ))
}

fn cohort_size_from_slots(
    template: QueryTemplate,
    slot_labels: &[String],
    slots: &[u64],
) -> Result<usize> {
    validate_slot_layout(template, slot_labels, slots.len())?;

    if let Some(size) = scalar::cohort_size_from_slots(template, slots) {
        return size;
    }
    if let Some(size) = grouped::cohort_size_from_slots(template, slot_labels, slots) {
        return size;
    }

    Err(anyhow!(
        "no cohort-size derivation configured for {}",
        template.as_str()
    ))
}

fn validate_slot_layout(
    template: QueryTemplate,
    slot_labels: &[String],
    slot_count: usize,
) -> Result<()> {
    if slot_labels.len() != slot_count {
        return Err(anyhow!(
            "slot label count does not match slot vector length"
        ));
    }

    if let Some(schema) = scalar::schema_for_query(template) {
        if schema.slot_labels != slot_labels {
            return Err(anyhow!(
                "statistics schema mismatch for {}",
                template.as_str()
            ));
        }
    }

    Ok(())
}

fn sensitivity_for(aggregated: &LocalStatistics, clip: ClipBounds) -> f64 {
    scalar::sensitivity(
        aggregated.template,
        &aggregated.slots,
        aggregated.cohort_size,
        clip,
    )
    .unwrap_or_else(|| helpers::clipped_mean_sensitivity(clip, aggregated.cohort_size))
}
