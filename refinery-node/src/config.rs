// src/config.rs
// Defines the configuration functions for the project.

// Standard library imports
use std::{env, fmt::Display, str::FromStr};

// Third-party library imports
use anyhow::{Result, anyhow};

// Local module imports
use crate::privacy::PrivacyConfig;

// Loads the environment variables from the `.env` file.
pub fn load_dotenv() {
    let _ = dotenvy::from_filename(".env");
}

// Loads the node secret from the environment variables.
pub fn load_node_secret() -> Result<String> {
    required_env("REFINERY_NODE_SECRET")
}

// Loads the privacy configuration from the environment variables.
pub fn load_privacy_config() -> Result<PrivacyConfig> {
    let config = PrivacyConfig {
        epsilon: parse_env("REFINERY_EPSILON")?,
        min_cohort: parse_env("REFINERY_MIN_COHORT")?,
        total_budget: parse_env("REFINERY_TOTAL_BUDGET")?,
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

    Ok(config)
}

// Loads a required environment variable.
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

// Parses an environment variable into a specific type.
fn parse_env<T>(name: &str) -> Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    let raw = required_env(name)?;
    raw.parse::<T>()
        .map_err(|err| anyhow!("failed to parse {name}={raw:?}: {err}"))
}
