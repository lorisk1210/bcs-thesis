// src/config.rs
// Defines orchestrator configuration loading from environment variables.

// Standard library imports
use std::env;
use std::path::PathBuf;

// Third-party library imports
use anyhow::{Result, anyhow};
use refinery_protocol::ReleaseMode;
use refinery_protocol::env_utils::{parse_env, parse_env_or_default, parse_optional_env};

// Global privacy settings applied after node aggregation.
#[derive(Debug, Clone)]
pub struct GlobalPrivacyConfig {
    pub epsilon: f64,
    pub min_cohort: usize,
    pub total_budget: f64,
    pub min_participating_nodes: usize,
    pub ledger_db_path: PathBuf,
    pub release_mode: ReleaseMode,
    pub dp_seed: Option<u64>,
}

// Loads environment variables from the local `.env` file.
pub fn load_dotenv() {
    let _ = dotenvy::from_filename(".env");
}

// Loads the orchestrator privacy configuration from environment variables.
// @return: Result<GlobalPrivacyConfig> - Parsed and validated privacy settings
pub fn load_privacy_config() -> Result<GlobalPrivacyConfig> {
    let config = GlobalPrivacyConfig {
        epsilon: parse_env("REFINERY_EPSILON")?,
        min_cohort: parse_env("REFINERY_MIN_COHORT")?,
        total_budget: parse_env("REFINERY_TOTAL_BUDGET")?,
        min_participating_nodes: parse_env_or_default("REFINERY_MIN_PARTICIPATING_NODES", 3)?,
        ledger_db_path: env::var("REFINERY_ORCHESTRATOR_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/orchestrator.duckdb")),
        release_mode: parse_env_or_default(ReleaseMode::ENV_NAME, ReleaseMode::Dp)?,
        dp_seed: parse_optional_env(ReleaseMode::DP_SEED_ENV_NAME)?,
    };

    if config.min_cohort == 0 {
        return Err(anyhow!("REFINERY_MIN_COHORT must be > 0"));
    }
    if config.release_mode.consumes_budget() {
        if config.epsilon <= 0.0 {
            return Err(anyhow!("REFINERY_EPSILON must be > 0"));
        }
        if config.total_budget <= 0.0 {
            return Err(anyhow!("REFINERY_TOTAL_BUDGET must be > 0"));
        }
    }
    if config.min_participating_nodes < 2 {
        return Err(anyhow!("REFINERY_MIN_PARTICIPATING_NODES must be >= 2"));
    }
    if config.release_mode.requires_seed() && config.dp_seed.is_none() {
        return Err(anyhow!(
            "{} must be set when {}=seeded",
            ReleaseMode::DP_SEED_ENV_NAME,
            ReleaseMode::ENV_NAME,
        ));
    }

    Ok(config)
}
