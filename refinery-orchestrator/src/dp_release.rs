// src/dp_release.rs
// Applies the final orchestrator-side differential privacy release step.

// Third-party library imports
use anyhow::{Result, anyhow};
use rand::{Rng, SeedableRng, rngs::StdRng};
use refinery_protocol::dp::{apply_noise, count_noised_metrics};
use serde::Serialize;

// Local module imports
use refinery_protocol::QueryResult;
use serde_json::Value;

use crate::config::GlobalPrivacyConfig;

// Result of the global release gate after aggregating node outputs.
#[derive(Debug, Clone, Serialize)]
pub struct GlobalReleaseResult {
    pub accepted: bool,
    pub reason: String,
    pub noisy_result: Option<Value>,
}

// Applies the final global release policy and injects DP noise when accepted.
// @param: query_result - Aggregated federated result before release
// @param: config - Global privacy settings for the orchestrator
// @return: Result<GlobalReleaseResult> - Release decision and optional noised payload
pub fn release_result(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
) -> Result<GlobalReleaseResult> {
    let mut rng = rand::thread_rng();
    release_result_with_rng(query_result, config, &mut rng)
}

// Applies the release policy using an explicit RNG for deterministic tests.
pub fn release_result_with_rng<R>(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
    rng: &mut R,
) -> Result<GlobalReleaseResult>
where
    R: Rng + ?Sized,
{
    if config.epsilon <= 0.0 {
        return Err(anyhow!("epsilon must be > 0"));
    }

    if query_result.cohort_size < config.min_cohort {
        return Ok(GlobalReleaseResult {
            accepted: false,
            reason: format!(
                "global cohort size {} is below minimum {}",
                query_result.cohort_size, config.min_cohort
            ),
            noisy_result: None,
        });
    }

    let mut noisy_result = query_result.raw_result.clone();
    let noised_metric_count = count_noised_metrics(&noisy_result).max(1);
    let epsilon_per_metric = config.epsilon / noised_metric_count as f64;
    let value_scale = if query_result.sensitivity <= 0.0 {
        0.0
    } else {
        query_result.sensitivity / epsilon_per_metric
    };
    let count_scale = 1.0 / epsilon_per_metric;
    apply_noise(&mut noisy_result, value_scale, count_scale, rng);

    Ok(GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        noisy_result: Some(noisy_result),
    })
}

// Deterministic helper for tests that need the exact same DP output across paths.
pub fn release_result_with_seed(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
    seed: u64,
) -> Result<GlobalReleaseResult> {
    let mut rng = StdRng::seed_from_u64(seed);
    release_result_with_rng(query_result, config, &mut rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn seeded_release_is_deterministic() {
        let query_result = QueryResult {
            template_name: "cohort_feasibility_count".to_string(),
            raw_result: json!({"count": 20}),
            cohort_size: 20,
            sensitivity: 1.0,
        };
        let config = GlobalPrivacyConfig {
            epsilon: 1.0,
            min_cohort: 1,
            total_budget: 10.0,
            min_participating_nodes: 3,
            ledger_db_path: "data/test_orchestrator.duckdb".into(),
        };

        let first =
            release_result_with_seed(&query_result, &config, 42).expect("seeded release works");
        let second =
            release_result_with_seed(&query_result, &config, 42).expect("seeded release works");
        assert_eq!(first.noisy_result, second.noisy_result);
        assert_eq!(first.reason, second.reason);
        assert_eq!(first.accepted, second.accepted);
    }
}
