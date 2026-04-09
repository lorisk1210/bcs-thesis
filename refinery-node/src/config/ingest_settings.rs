use std::env;

use anyhow::{Result, anyhow};

use crate::ingest::TransformMode;

const DISABLE_DATA_COARSENING_ENV_NAME: &str = "REFINERY_DISABLE_DATA_COARSENING";

pub fn load_ingest_transform_mode() -> Result<TransformMode> {
    let raw = match env::var(DISABLE_DATA_COARSENING_ENV_NAME) {
        Ok(raw) => Some(raw),
        Err(env::VarError::NotPresent) => None,
        Err(err) => {
            return Err(anyhow!(
                "failed to read {DISABLE_DATA_COARSENING_ENV_NAME}: {err}"
            ));
        }
    };

    resolve_ingest_transform_mode(raw.as_deref())
}

pub fn resolve_ingest_transform_mode(raw: Option<&str>) -> Result<TransformMode> {
    let disable_coarsening = parse_bool_flag(raw, DISABLE_DATA_COARSENING_ENV_NAME)?;
    Ok(if disable_coarsening {
        TransformMode::Exact
    } else {
        TransformMode::Coarsened
    })
}

fn parse_bool_flag(raw: Option<&str>, env_name: &str) -> Result<bool> {
    let Some(raw) = raw else {
        return Ok(false);
    };

    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(anyhow!("{env_name} is set but empty"));
    }

    match normalized.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(anyhow!(
            "failed to parse {env_name}={raw:?}: expected one of true/false, 1/0, yes/no, or on/off"
        )),
    }
}
