use std::fmt::{self, Display};
use std::str::FromStr;

use anyhow::{Result, anyhow};
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::dp::{apply_noise, count_noised_metrics, sample_laplace};
use crate::{QueryResult, QueryTemplate};

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

fn required_numeric_field(payload: &Value, key: &str) -> Result<f64> {
    payload
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing numeric field '{key}'"))
}
