// src/dp_release.rs
// Applies the final orchestrator-side differential privacy release step.

// Third-party library imports
use anyhow::{Result, anyhow};
use serde::Serialize;

// Local module imports
use refinery_protocol::{
    QueryResult, ReleaseMode, release_query_result, release_query_result_with_rng,
};
use serde_json::Value;

use crate::config::GlobalPrivacyConfig;

// Result of the global release gate after aggregating node outputs.
#[derive(Debug, Clone, Serialize)]
pub struct GlobalReleaseResult {
    pub accepted: bool,
    pub reason: String,
    pub release_mode: ReleaseMode,
    pub released_result: Option<Value>,
}

// Applies the final global release policy and injects DP noise when accepted.
// @param: query_result - Aggregated federated result before release
// @param: config - Global privacy settings for the orchestrator
// @return: Result<GlobalReleaseResult> - Release decision and optional noised payload
pub fn release_result(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
) -> Result<GlobalReleaseResult> {
    release_result_for_mode(query_result, config, config.release_mode, config.dp_seed)
}

// Applies the release policy using an explicit RNG for deterministic tests.
pub fn release_result_with_rng<R>(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
    rng: &mut R,
) -> Result<GlobalReleaseResult>
where
    R: rand::Rng + ?Sized,
{
    release_result_with_rng_for_mode(query_result, config, config.release_mode, rng)
}

// Deterministic helper for tests that need the exact same DP output across paths.
pub fn release_result_with_seed(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
    seed: u64,
) -> Result<GlobalReleaseResult> {
    release_result_for_mode(query_result, config, ReleaseMode::Seeded, Some(seed))
}

fn release_result_for_mode(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
    release_mode: ReleaseMode,
    dp_seed: Option<u64>,
) -> Result<GlobalReleaseResult> {
    if release_mode.consumes_budget() && config.epsilon <= 0.0 {
        return Err(anyhow!("epsilon must be > 0"));
    }

    if query_result.cohort_size < config.min_cohort {
        return Ok(GlobalReleaseResult {
            accepted: false,
            reason: format!(
                "global cohort size {} is below minimum {}",
                query_result.cohort_size, config.min_cohort
            ),
            release_mode,
            released_result: None,
        });
    }

    let released_result =
        release_query_result(query_result, config.epsilon, release_mode, dp_seed)?;

    Ok(GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        release_mode,
        released_result: Some(released_result),
    })
}

fn release_result_with_rng_for_mode<R>(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
    release_mode: ReleaseMode,
    rng: &mut R,
) -> Result<GlobalReleaseResult>
where
    R: rand::Rng + ?Sized,
{
    if release_mode.consumes_budget() && config.epsilon <= 0.0 {
        return Err(anyhow!("epsilon must be > 0"));
    }

    if query_result.cohort_size < config.min_cohort {
        return Ok(GlobalReleaseResult {
            accepted: false,
            reason: format!(
                "global cohort size {} is below minimum {}",
                query_result.cohort_size, config.min_cohort
            ),
            release_mode,
            released_result: None,
        });
    }

    let released_result = if release_mode == ReleaseMode::Raw {
        query_result.raw_result.clone()
    } else {
        release_query_result_with_rng(query_result, config.epsilon, rng)?
    };

    Ok(GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        release_mode,
        released_result: Some(released_result),
    })
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
            release_mode: ReleaseMode::Dp,
            dp_seed: None,
        };

        let first =
            release_result_with_seed(&query_result, &config, 42).expect("seeded release works");
        let second =
            release_result_with_seed(&query_result, &config, 42).expect("seeded release works");
        assert_eq!(first.released_result, second.released_result);
        assert_eq!(first.reason, second.reason);
        assert_eq!(first.accepted, second.accepted);
    }

    #[test]
    fn raw_release_returns_exact_payload() {
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
            release_mode: ReleaseMode::Raw,
            dp_seed: None,
        };

        let release = release_result(&query_result, &config).expect("raw release should work");
        assert!(release.accepted);
        assert_eq!(release.release_mode, ReleaseMode::Raw);
        assert_eq!(release.released_result, Some(query_result.raw_result));
    }
}
