use std::fmt::{self, Display};
use std::str::FromStr;

use anyhow::{Result, anyhow};
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::dp::{apply_noise, count_noised_metrics, sample_laplace};
use crate::{ClipBounds, QueryResult, QueryTemplate};

const SUBGROUP_SUM_EPSILON_SHARE: f64 = 0.8;
const SUBGROUP_COUNT_EPSILON_SHARE: f64 = 0.2;
const DOSE_RESPONSE_SUM_EPSILON_SHARE: f64 = 0.8;
const DOSE_RESPONSE_COUNT_EPSILON_SHARE: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseMode {
    Dp,
    Raw,
    Seeded,
}

impl ReleaseMode {
    pub const ENV_NAME: &str = "REFINERY_RELEASE_MODE";
    pub const DP_SEED_ENV_NAME: &str = "REFINERY_DP_SEED";

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dp => "dp",
            Self::Raw => "raw",
            Self::Seeded => "seeded",
        }
    }

    pub fn consumes_budget(self) -> bool {
        !matches!(self, Self::Raw)
    }

    pub fn requires_seed(self) -> bool {
        matches!(self, Self::Seeded)
    }
}

impl Default for ReleaseMode {
    fn default() -> Self {
        Self::Dp
    }
}

impl Display for ReleaseMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReleaseMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim() {
            "dp" => Ok(Self::Dp),
            "raw" => Ok(Self::Raw),
            "seeded" => Ok(Self::Seeded),
            other => Err(anyhow!(
                "invalid {}={other:?}; expected one of: dp, raw, seeded",
                Self::ENV_NAME,
            )),
        }
    }
}

pub fn release_query_result(
    query_result: &QueryResult,
    epsilon: f64,
    mode: ReleaseMode,
    dp_seed: Option<u64>,
) -> Result<Value> {
    match mode {
        ReleaseMode::Dp => {
            let mut rng = rand::thread_rng();
            release_query_result_with_rng(query_result, epsilon, &mut rng)
        }
        ReleaseMode::Raw => Ok(query_result.raw_result.clone()),
        ReleaseMode::Seeded => {
            let seed = dp_seed.ok_or_else(|| {
                anyhow!(
                    "{} must be set when {}=seeded",
                    ReleaseMode::DP_SEED_ENV_NAME,
                    ReleaseMode::ENV_NAME,
                )
            })?;
            let mut rng = StdRng::seed_from_u64(seed);
            release_query_result_with_rng(query_result, epsilon, &mut rng)
        }
    }
}

pub fn grouped_release_rejection_reason(
    query_result: &QueryResult,
    min_cohort: usize,
) -> Result<Option<String>> {
    let (group_key, group_kind) = if query_result.template_name
        == QueryTemplate::SubgroupEffectEstimate.as_str()
    {
        ("subgroup", "subgroups")
    } else if query_result.template_name == QueryTemplate::DoseResponseTrend.as_str() {
        ("dose_bucket", "dose buckets")
    } else {
        return Ok(None);
    };

    let grouped_stats = query_result
        .dp_release_stats
        .as_ref()
        .unwrap_or(&query_result.raw_result);
    let groups = grouped_stats
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing groups array in grouped release stats"))?;

    let mut underpowered = Vec::new();
    for group in groups {
        let label = required_string_field(group, group_key)?;
        let n = required_numeric_field(group, "n")?;
        if n < min_cohort as f64 {
            underpowered.push(format!("{label}={n:.0}"));
        }
    }

    if underpowered.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!(
            "grouped result is inconclusive because one or more {group_kind} are below minimum {min_cohort}: {}",
            underpowered.join(", ")
        )))
    }
}

pub fn release_query_result_with_rng<R>(
    query_result: &QueryResult,
    epsilon: f64,
    rng: &mut R,
) -> Result<Value>
where
    R: Rng + ?Sized,
{
    if epsilon <= 0.0 {
        return Err(anyhow!("epsilon must be > 0"));
    }

    if query_result.template_name == QueryTemplate::CohortFeasibilityCount.as_str() {
        return release_cohort_feasibility_with_rng(query_result, epsilon, rng);
    }
    if query_result.template_name == QueryTemplate::SubgroupEffectEstimate.as_str() {
        return release_subgroup_effect_with_rng(query_result, epsilon, rng);
    }
    if query_result.template_name == QueryTemplate::DoseResponseTrend.as_str() {
        return release_dose_response_with_rng(query_result, epsilon, rng);
    }

    let mut released_result = query_result.raw_result.clone();
    let noised_metric_count = count_noised_metrics(&released_result).max(1);
    let epsilon_per_metric = epsilon / noised_metric_count as f64;
    let value_scale = if query_result.sensitivity <= 0.0 {
        0.0
    } else {
        query_result.sensitivity / epsilon_per_metric
    };
    let count_scale = 1.0 / epsilon_per_metric;
    apply_noise(&mut released_result, value_scale, count_scale, rng);
    Ok(released_result)
}

fn release_cohort_feasibility_with_rng<R>(
    query_result: &QueryResult,
    epsilon: f64,
    rng: &mut R,
) -> Result<Value>
where
    R: Rng + ?Sized,
{
    let exact_count = required_numeric_field(&query_result.raw_result, "count")?;
    let exact_population = required_numeric_field(&query_result.raw_result, "population_in_scope")?;
    let epsilon_per_metric = epsilon / 2.0;
    let count_scale = 1.0 / epsilon_per_metric;

    let mut noisy_count = (exact_count + sample_laplace(count_scale, rng)).max(0.0);
    let noisy_population = (exact_population + sample_laplace(count_scale, rng)).max(0.0);
    if noisy_count > noisy_population {
        noisy_count = noisy_population;
    }
    let prevalence =
        (noisy_population > f64::EPSILON).then(|| (noisy_count / noisy_population).clamp(0.0, 1.0));

    Ok(json!({
        "count": noisy_count,
        "population_in_scope": noisy_population,
        "prevalence": prevalence,
    }))
}

fn release_subgroup_effect_with_rng<R>(
    query_result: &QueryResult,
    epsilon: f64,
    rng: &mut R,
) -> Result<Value>
where
    R: Rng + ?Sized,
{
    let stats = query_result
        .dp_release_stats
        .as_ref()
        .ok_or_else(|| anyhow!("missing internal stats for subgroup_effect_estimate release"))?;
    let clip = query_result
        .clip_bounds
        .ok_or_else(|| anyhow!("missing clip bounds for subgroup_effect_estimate release"))?;
    let groups = stats
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing groups array in subgroup_effect_estimate stats"))?;
    let clip_range = (clip.max - clip.min).abs();
    let sum_scale = if clip_range <= 0.0 {
        0.0
    } else {
        clip_range / (epsilon * SUBGROUP_SUM_EPSILON_SHARE)
    };
    let count_scale = 1.0 / (epsilon * SUBGROUP_COUNT_EPSILON_SHARE);

    let mut released_groups = Vec::with_capacity(groups.len());
    for group in groups {
        let subgroup = required_string_field(group, "subgroup")?;
        let exact_n = required_numeric_field(group, "n")?;
        let exact_sum = required_numeric_field(group, "outcome_sum")?;
        let noisy_n = (exact_n + sample_laplace(count_scale, rng)).max(0.0);
        let mean_outcome = if noisy_n <= f64::EPSILON {
            Value::Null
        } else {
            let noisy_sum = exact_sum + sample_laplace(sum_scale, rng);
            Value::from(clamp_to_bounds(noisy_sum / noisy_n, clip))
        };
        released_groups.push(json!({
            "subgroup": subgroup,
            "mean_outcome": mean_outcome
        }));
    }

    Ok(json!({ "groups": released_groups }))
}

fn release_dose_response_with_rng<R>(
    query_result: &QueryResult,
    epsilon: f64,
    rng: &mut R,
) -> Result<Value>
where
    R: Rng + ?Sized,
{
    let stats = query_result
        .dp_release_stats
        .as_ref()
        .ok_or_else(|| anyhow!("missing internal stats for dose_response_trend release"))?;
    let clip = query_result
        .clip_bounds
        .ok_or_else(|| anyhow!("missing clip bounds for dose_response_trend release"))?;
    let groups = stats
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing groups array in dose_response_trend stats"))?;
    let clip_range = (clip.max - clip.min).abs();
    let sum_scale = if clip_range <= 0.0 {
        0.0
    } else {
        clip_range / (epsilon * DOSE_RESPONSE_SUM_EPSILON_SHARE)
    };
    let count_scale = 1.0 / (epsilon * DOSE_RESPONSE_COUNT_EPSILON_SHARE);

    let mut released_groups = Vec::with_capacity(groups.len());
    for group in groups {
        let dose_bucket = required_string_field(group, "dose_bucket")?;
        let exact_n = required_numeric_field(group, "n")?;
        let exact_sum = required_numeric_field(group, "outcome_sum")?;
        let noisy_n = (exact_n + sample_laplace(count_scale, rng)).max(0.0);
        let mean_outcome = if noisy_n <= f64::EPSILON {
            Value::Null
        } else {
            let noisy_sum = exact_sum + sample_laplace(sum_scale, rng);
            Value::from(clamp_to_bounds(noisy_sum / noisy_n, clip))
        };
        released_groups.push(json!({
            "dose_bucket": dose_bucket,
            "n": noisy_n,
            "mean_outcome": mean_outcome
        }));
    }

    Ok(json!({ "groups": released_groups }))
}

fn clamp_to_bounds(value: f64, clip: ClipBounds) -> f64 {
    let lower = clip.min.min(clip.max);
    let upper = clip.min.max(clip.max);
    value.clamp(lower, upper)
}

fn required_numeric_field(payload: &Value, key: &str) -> Result<f64> {
    payload
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing numeric field '{key}'"))
}

fn required_string_field<'a>(payload: &'a Value, key: &str) -> Result<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string field '{key}'"))
}
