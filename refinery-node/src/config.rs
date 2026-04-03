// src/config.rs
// Defines the configuration functions for the project.

// Standard library imports
use std::env;

// Third-party library imports
use anyhow::{Result, anyhow};
use refinery_protocol::env_utils::{
    parse_env, parse_env_or_default, parse_optional_env, required_env,
};
use refinery_protocol::ReleaseMode;

// Local module imports
use crate::privacy::PrivacyConfig;

// SMPC configuration derived from environment variables.
#[derive(Debug, Clone)]
pub struct SmpcConfig {
    pub private_key_bytes: Option<[u8; 32]>,
    pub min_participating_nodes: usize,
}

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
    if config.release_mode.requires_seed() && config.dp_seed.is_none() {
        return Err(anyhow!(
            "{} must be set when {}=seeded",
            ReleaseMode::DP_SEED_ENV_NAME,
            ReleaseMode::ENV_NAME,
        ));
    }

    Ok(config)
}

// Loads the node SMPC configuration from environment variables.
pub fn load_smpc_config() -> Result<SmpcConfig> {
    let private_key_bytes = match env::var("REFINERY_SMPC_PRIVATE_KEY_HEX") {
        Ok(value) => {
            let decoded = hex::decode(value.trim())
                .map_err(|err| anyhow!("failed to decode REFINERY_SMPC_PRIVATE_KEY_HEX: {err}"))?;
            Some(refinery_protocol::validate_private_key_bytes(&decoded)?)
        }
        Err(env::VarError::NotPresent) => None,
        Err(err) => return Err(anyhow!("failed to read REFINERY_SMPC_PRIVATE_KEY_HEX: {err}")),
    };

    let min_participating_nodes = match env::var("REFINERY_MIN_PARTICIPATING_NODES") {
        Ok(value) => {
            let parsed = value.trim().parse::<usize>().map_err(|err| {
                anyhow!(
                    "failed to parse REFINERY_MIN_PARTICIPATING_NODES={:?}: {err}",
                    value
                )
            })?;
            if parsed < 2 {
                return Err(anyhow!(
                    "REFINERY_MIN_PARTICIPATING_NODES must be >= 2"
                ));
            }
            parsed
        }
        Err(env::VarError::NotPresent) => 3,
        Err(err) => {
            return Err(anyhow!(
                "failed to read REFINERY_MIN_PARTICIPATING_NODES: {err}"
            ))
        }
    };

    Ok(SmpcConfig {
        private_key_bytes,
        min_participating_nodes,
    })
}
