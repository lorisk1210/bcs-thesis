use std::env;
use std::fmt::Display;
use std::str::FromStr;

use anyhow::{Result, anyhow};

pub fn required_env(name: &str) -> Result<String> {
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

pub fn parse_env<T>(name: &str) -> Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    let raw = required_env(name)?;
    raw.parse::<T>()
        .map_err(|err| anyhow!("failed to parse {name}={raw:?}: {err}"))
}

pub fn parse_env_or_default<T>(name: &str, default: T) -> Result<T>
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

pub fn parse_optional_env<T>(name: &str) -> Result<Option<T>>
where
    T: FromStr,
    T::Err: Display,
{
    match env::var(name) {
        Ok(raw) => {
            let raw = raw.trim();
            if raw.is_empty() {
                return Err(anyhow!("{name} is set but empty"));
            }
            let parsed = raw
                .parse::<T>()
                .map_err(|err| anyhow!("failed to parse {name}={raw:?}: {err}"))?;
            Ok(Some(parsed))
        }
        Err(env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(anyhow!("failed to read {name}: {err}")),
    }
}
