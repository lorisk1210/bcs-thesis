// src/config.rs
// Defines orchestrator configuration loading from environment variables.

// Standard library imports
use std::env;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

// Third-party library imports
use anyhow::{Result, anyhow};

// Global privacy settings applied after node aggregation.
#[derive(Debug, Clone)]
pub struct GlobalPrivacyConfig {
    pub epsilon: f64,
    pub min_cohort: usize,
    pub total_budget: f64,
    pub min_participating_nodes: usize,
    pub ledger_db_path: PathBuf,
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
    };

    if config.epsilon <= 0.0 {
        return Err(anyhow!("REFINERY_EPSILON must be > 0"));
    }
    if config.min_cohort == 0 {
        return Err(anyhow!("REFINERY_MIN_COHORT must be > 0"));
    }
    if config.total_budget <= 0.0 {
        return Err(anyhow!("REFINERY_TOTAL_BUDGET must be > 0"));
    }
    if config.min_participating_nodes < 2 {
        return Err(anyhow!("REFINERY_MIN_PARTICIPATING_NODES must be >= 2"));
    }

    Ok(config)
}

// Loads a required environment variable.
// @param: name - Environment variable name
// @return: Result<String> - Trimmed non-empty value
fn required_env(name: &str) -> Result<String> {
    match env::var(name) {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() {
                Err(anyhow!("{name} is set but empty"))
            } else {
                Ok(value.to_string())
            }
        }
        Err(env::VarError::NotPresent) => Err(anyhow!("{name} is not set")),
        Err(err) => Err(anyhow!("failed to read {name}: {err}")),
    }
}

// Parses an environment variable into a typed value.
// @param: name - Environment variable name
// @return: Result<T> - Parsed typed configuration value
fn parse_env<T>(name: &str) -> Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    let raw = required_env(name)?;
    raw.parse::<T>()
        .map_err(|err| anyhow!("failed to parse {name}={raw:?}: {err}"))
}

fn parse_env_or_default<T>(name: &str, default: T) -> Result<T>
where
    T: FromStr + Copy,
    T::Err: Display,
{
    match env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<T>()
            .map_err(|err| anyhow!("failed to parse {name}={raw:?}: {err}")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(err) => Err(anyhow!("failed to read {name}: {err}")),
    }
}
