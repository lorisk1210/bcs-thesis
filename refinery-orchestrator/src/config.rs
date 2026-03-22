use std::env;
use std::fmt::Display;
use std::str::FromStr;

use anyhow::{Result, anyhow};

#[derive(Debug, Clone)]
pub struct GlobalPrivacyConfig {
    pub epsilon: f64,
    pub min_cohort: usize,
    pub total_budget: f64,
}

pub fn load_dotenv() {
    let _ = dotenvy::from_filename(".env");
}

pub fn load_privacy_config() -> Result<GlobalPrivacyConfig> {
    let config = GlobalPrivacyConfig {
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

fn parse_env<T>(name: &str) -> Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    let raw = required_env(name)?;
    raw.parse::<T>()
        .map_err(|err| anyhow!("failed to parse {name}={raw:?}: {err}"))
}
