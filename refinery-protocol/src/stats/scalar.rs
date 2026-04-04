// stats/scalar.rs
// Scalar-template schema, codec, and rendering helpers.

// Third-party library imports
use anyhow::{Result, anyhow};
use serde_json::{Map, Value, json};

// Local module imports
use super::StatisticsSchema;
use super::encoding::{
    decode_count, decode_fixed, decode_signed, encode_count, encode_fixed, encode_signed,
};
use super::helpers::{
    clipped_mean_sensitivity, required_f64, required_i64, required_u64, safe_mean, safe_rate,
};
use crate::query::{ClipBounds, QueryTemplate};

const TWO_ARM_COUNT_SLOTS: &[usize] = &[0, 1];

const COHORT_FEASIBILITY_FIELDS: &[ScalarFieldSpec] = &[ScalarFieldSpec::count("count")];
const COMPARATIVE_EFFECTIVENESS_FIELDS: &[ScalarFieldSpec] = &[
    ScalarFieldSpec::count("n_exposed"),
    ScalarFieldSpec::count("n_control"),
    ScalarFieldSpec::fixed("outcome_sum_exposed"),
    ScalarFieldSpec::fixed("outcome_sum_control"),
];
const TIME_TO_EVENT_FIELDS: &[ScalarFieldSpec] = &[
    ScalarFieldSpec::count("n"),
    ScalarFieldSpec::fixed("sum_days_to_event"),
    ScalarFieldSpec::signed("max_days", Some(3650)),
];
const AE_INCIDENCE_FIELDS: &[ScalarFieldSpec] = &[
    ScalarFieldSpec::count("n_exposed"),
    ScalarFieldSpec::count("n_control"),
    ScalarFieldSpec::fixed("ae_count_exposed"),
    ScalarFieldSpec::fixed("ae_count_control"),
];
const DDI_INCIDENCE_FIELDS: &[ScalarFieldSpec] = &[
    ScalarFieldSpec::count("n_combo"),
    ScalarFieldSpec::count("n_a_only"),
    ScalarFieldSpec::fixed("ae_count_combo"),
    ScalarFieldSpec::fixed("ae_count_a_only"),
];

const COHORT_FEASIBILITY_SPEC: ScalarTemplateSpec = ScalarTemplateSpec {
    fields: COHORT_FEASIBILITY_FIELDS,
    render: ScalarRenderKind::Count,
    cohort_size: ScalarCohortSize::Single(0),
    sensitivity: ScalarSensitivityKind::Count,
};
const COMPARATIVE_EFFECTIVENESS_SPEC: ScalarTemplateSpec = ScalarTemplateSpec {
    fields: COMPARATIVE_EFFECTIVENESS_FIELDS,
    render: ScalarRenderKind::ComparativeDelta,
    cohort_size: ScalarCohortSize::Sum(TWO_ARM_COUNT_SLOTS),
    sensitivity: ScalarSensitivityKind::ClippedMean,
};
const TIME_TO_EVENT_SPEC: ScalarTemplateSpec = ScalarTemplateSpec {
    fields: TIME_TO_EVENT_FIELDS,
    render: ScalarRenderKind::Mean {
        mean_label: "mean_days_to_event",
    },
    cohort_size: ScalarCohortSize::Single(0),
    sensitivity: ScalarSensitivityKind::TimeToEvent { max_days_index: 2 },
};
const AE_INCIDENCE_SPEC: ScalarTemplateSpec = ScalarTemplateSpec {
    fields: AE_INCIDENCE_FIELDS,
    render: ScalarRenderKind::Incidence {
        left_output_label: "incidence_exposed",
        right_output_label: "incidence_control",
    },
    cohort_size: ScalarCohortSize::Sum(TWO_ARM_COUNT_SLOTS),
    sensitivity: ScalarSensitivityKind::InverseCount,
};
const DDI_INCIDENCE_SPEC: ScalarTemplateSpec = ScalarTemplateSpec {
    fields: DDI_INCIDENCE_FIELDS,
    render: ScalarRenderKind::Incidence {
        left_output_label: "incidence_combo",
        right_output_label: "incidence_a_only",
    },
    cohort_size: ScalarCohortSize::Sum(TWO_ARM_COUNT_SLOTS),
    sensitivity: ScalarSensitivityKind::InverseCount,
};

#[derive(Clone, Copy)]
struct ScalarTemplateSpec {
    fields: &'static [ScalarFieldSpec],
    render: ScalarRenderKind,
    cohort_size: ScalarCohortSize,
    sensitivity: ScalarSensitivityKind,
}

#[derive(Clone, Copy)]
struct ScalarFieldSpec {
    label: &'static str,
    codec: ScalarCodec,
    default_signed: Option<i64>,
}

impl ScalarFieldSpec {
    const fn count(label: &'static str) -> Self {
        Self {
            label,
            codec: ScalarCodec::Count,
            default_signed: None,
        }
    }

    const fn fixed(label: &'static str) -> Self {
        Self {
            label,
            codec: ScalarCodec::Fixed,
            default_signed: None,
        }
    }

    const fn signed(label: &'static str, default_signed: Option<i64>) -> Self {
        Self {
            label,
            codec: ScalarCodec::Signed,
            default_signed,
        }
    }
}

#[derive(Clone, Copy)]
enum ScalarCodec {
    Count,
    Fixed,
    Signed,
}

#[derive(Clone, Copy)]
enum ScalarRenderKind {
    Count,
    ComparativeDelta,
    Mean {
        mean_label: &'static str,
    },
    Incidence {
        left_output_label: &'static str,
        right_output_label: &'static str,
    },
}

#[derive(Clone, Copy)]
enum ScalarCohortSize {
    Single(usize),
    Sum(&'static [usize]),
}

#[derive(Clone, Copy)]
enum ScalarSensitivityKind {
    Count,
    ClippedMean,
    TimeToEvent { max_days_index: usize },
    InverseCount,
}

pub(crate) fn schema_for_query(template: QueryTemplate) -> Option<StatisticsSchema> {
    scalar_template_spec(template).map(|spec| StatisticsSchema {
        template,
        schema_id: format!("{}:v1", template.as_str()),
        slot_labels: spec
            .fields
            .iter()
            .map(|field| field.label.to_string())
            .collect(),
    })
}

pub(crate) fn encode_stats(template: QueryTemplate, stats: &Value) -> Option<Result<Vec<u64>>> {
    scalar_template_spec(template).map(|spec| {
        spec.fields
            .iter()
            .map(|field| match field.codec {
                ScalarCodec::Count => encode_count(required_u64(stats, field.label)?),
                ScalarCodec::Fixed => encode_fixed(required_f64(stats, field.label)?),
                ScalarCodec::Signed => {
                    encode_signed(required_i64(stats, field.label, field.default_signed)?)
                }
            })
            .collect()
    })
}

pub(crate) fn decode_stats(template: QueryTemplate, slots: &[u64]) -> Option<Result<Value>> {
    scalar_template_spec(template).map(|spec| {
        let mut map = Map::new();
        for (field, slot) in spec.fields.iter().zip(slots.iter()) {
            let value = match field.codec {
                ScalarCodec::Count => Value::from(decode_count(*slot)?),
                ScalarCodec::Fixed => Value::from(decode_fixed(*slot)?),
                ScalarCodec::Signed => Value::from(decode_signed(*slot)?),
            };
            map.insert(field.label.to_string(), value);
        }
        Ok(Value::Object(map))
    })
}

pub(crate) fn render_result(template: QueryTemplate, stats: &Value) -> Option<Result<Value>> {
    scalar_template_spec(template).map(|spec| match spec.render {
        ScalarRenderKind::Count => Ok(json!({
            spec.fields[0].label: required_u64(stats, spec.fields[0].label)?
        })),
        ScalarRenderKind::ComparativeDelta => {
            let left_n = required_u64(stats, spec.fields[0].label)?;
            let right_n = required_u64(stats, spec.fields[1].label)?;
            let left_sum = required_f64(stats, spec.fields[2].label)?;
            let right_sum = required_f64(stats, spec.fields[3].label)?;
            let left_mean = safe_mean(left_sum, left_n);
            let right_mean = safe_mean(right_sum, right_n);
            Ok(json!({
                spec.fields[0].label: left_n,
                spec.fields[1].label: right_n,
                "mean_outcome_exposed": left_mean,
                "mean_outcome_control": right_mean,
                "delta": match (left_mean, right_mean) {
                    (Some(left), Some(right)) => Some(left - right),
                    _ => None,
                }
            }))
        }
        ScalarRenderKind::Mean { mean_label } => {
            let count = required_u64(stats, spec.fields[0].label)?;
            let sum = required_f64(stats, spec.fields[1].label)?;
            Ok(json!({
                spec.fields[0].label: count,
                mean_label: safe_mean(sum, count)
            }))
        }
        ScalarRenderKind::Incidence {
            left_output_label,
            right_output_label,
        } => {
            let left_n = required_u64(stats, spec.fields[0].label)?;
            let right_n = required_u64(stats, spec.fields[1].label)?;
            let left_count = required_f64(stats, spec.fields[2].label)?;
            let right_count = required_f64(stats, spec.fields[3].label)?;
            Ok(json!({
                spec.fields[0].label: left_n,
                spec.fields[1].label: right_n,
                left_output_label: safe_rate(left_count, left_n),
                right_output_label: safe_rate(right_count, right_n)
            }))
        }
    })
}

pub(crate) fn cohort_size_from_slots(
    template: QueryTemplate,
    slots: &[u64],
) -> Option<Result<usize>> {
    scalar_template_spec(template).map(|spec| match spec.cohort_size {
        ScalarCohortSize::Single(index) => Ok(decode_count(slots[index])? as usize),
        ScalarCohortSize::Sum(indices) => indices.iter().try_fold(0usize, |total, index| {
            total
                .checked_add(decode_count(slots[*index])? as usize)
                .ok_or_else(|| anyhow!("cohort size overflow"))
        }),
    })
}

pub(crate) fn sensitivity(
    template: QueryTemplate,
    slots: &[u64],
    cohort_size: usize,
    clip: ClipBounds,
) -> Option<f64> {
    scalar_template_spec(template).map(|spec| match spec.sensitivity {
        ScalarSensitivityKind::Count => 1.0,
        ScalarSensitivityKind::ClippedMean => clipped_mean_sensitivity(clip, cohort_size),
        ScalarSensitivityKind::TimeToEvent { max_days_index } => {
            let max_days = slots
                .get(max_days_index)
                .copied()
                .and_then(|slot| decode_signed(slot).ok())
                .unwrap_or(3650) as f64;
            max_days / cohort_size.max(1) as f64
        }
        ScalarSensitivityKind::InverseCount => inverse_count_sensitivity(cohort_size),
    })
}

pub(crate) fn normalize_aggregated_slots(
    template: QueryTemplate,
    slots: &mut [u64],
    participant_count: usize,
) -> Option<Result<()>> {
    scalar_template_spec(template).map(|spec| match spec.sensitivity {
        ScalarSensitivityKind::TimeToEvent { max_days_index } => {
            normalize_invariant_signed_slot(slots, max_days_index, participant_count, "max_days")
        }
        _ => Ok(()),
    })
}

fn scalar_template_spec(template: QueryTemplate) -> Option<&'static ScalarTemplateSpec> {
    match template {
        QueryTemplate::CohortFeasibilityCount => Some(&COHORT_FEASIBILITY_SPEC),
        QueryTemplate::ComparativeEffectivenessDelta => Some(&COMPARATIVE_EFFECTIVENESS_SPEC),
        QueryTemplate::TimeToEventProxy => Some(&TIME_TO_EVENT_SPEC),
        QueryTemplate::AeIncidenceSignalProxy => Some(&AE_INCIDENCE_SPEC),
        QueryTemplate::DdiSignalProxy => Some(&DDI_INCIDENCE_SPEC),
        QueryTemplate::SubgroupEffectEstimate | QueryTemplate::DoseResponseTrend => None,
    }
}

fn inverse_count_sensitivity(cohort_size: usize) -> f64 {
    1.0 / cohort_size.max(1) as f64
}

fn normalize_invariant_signed_slot(
    slots: &mut [u64],
    index: usize,
    participant_count: usize,
    label: &str,
) -> Result<()> {
    if participant_count == 0 {
        return Err(anyhow!("participant count must be > 0"));
    }
    let value = slots
        .get(index)
        .copied()
        .ok_or_else(|| anyhow!("missing invariant slot {label}"))?;
    let decoded = decode_signed(value)?;
    let divisor =
        i64::try_from(participant_count).map_err(|_| anyhow!("participant count overflow"))?;
    if decoded % divisor != 0 {
        return Err(anyhow!("aggregated invariant slot {label} is inconsistent"));
    }
    slots[index] = encode_signed(decoded / divisor)?;
    Ok(())
}
