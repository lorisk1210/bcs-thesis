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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn release_mode_parses_expected_values() {
        assert_eq!(
            "dp".parse::<ReleaseMode>().expect("dp should parse"),
            ReleaseMode::Dp
        );
        assert_eq!(
            "raw".parse::<ReleaseMode>().expect("raw should parse"),
            ReleaseMode::Raw
        );
        assert_eq!(
            "seeded"
                .parse::<ReleaseMode>()
                .expect("seeded should parse"),
            ReleaseMode::Seeded
        );
    }

    #[test]
    fn raw_release_returns_exact_payload() {
        let query_result = QueryResult {
            template_name: "cohort_feasibility_count".to_string(),
            raw_result: json!({"count": 12, "population_in_scope": 30, "prevalence": 0.4}),
            cohort_size: 12,
            sensitivity: 1.0,
        };

        let released = release_query_result(&query_result, 1.0, ReleaseMode::Raw, None)
            .expect("raw release should work");
        assert_eq!(released, query_result.raw_result);
    }

    #[test]
    fn seeded_release_is_deterministic() {
        let query_result = QueryResult {
            template_name: "comparative_effectiveness_delta".to_string(),
            raw_result: json!({"count": 12, "delta": 0.5}),
            cohort_size: 12,
            sensitivity: 1.0,
        };

        let first = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7))
            .expect("seeded release should work");
        let second = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7))
            .expect("seeded release should work");
        assert_eq!(first, second);
    }

    #[test]
    fn seeded_release_requires_seed() {
        let query_result = QueryResult {
            template_name: "cohort_feasibility_count".to_string(),
            raw_result: json!({"count": 12, "population_in_scope": 30, "prevalence": 0.4}),
            cohort_size: 12,
            sensitivity: 1.0,
        };

        let error =
            release_query_result(&query_result, 1.0, ReleaseMode::Seeded, None).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("REFINERY_DP_SEED must be set when REFINERY_RELEASE_MODE=seeded")
        );
    }

    #[test]
    fn feasibility_release_derives_prevalence_from_noised_counts() {
        let query_result = QueryResult {
            template_name: "cohort_feasibility_count".to_string(),
            raw_result: json!({"count": 12, "population_in_scope": 30, "prevalence": 0.4}),
            cohort_size: 12,
            sensitivity: 1.0,
        };

        let released = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7))
            .expect("seeded release should work");
        let count = released["count"].as_f64().expect("count should be numeric");
        let population = released["population_in_scope"]
            .as_f64()
            .expect("population should be numeric");
        let prevalence = released["prevalence"]
            .as_f64()
            .expect("prevalence should be numeric");

        assert!(count >= 0.0);
        assert!(population >= 0.0);
        assert!(count <= population + 1e-12);
        if population > f64::EPSILON {
            assert!((prevalence - (count / population)).abs() < 1e-12);
        }
    }
}
