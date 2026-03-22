// src/federation.rs
// Shared federation mode definitions used by the node and orchestrator.

// Standard library imports
use std::str::FromStr;

// Third-party library imports
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

// Supported federation modes for orchestrated execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FederationMode {
    Plaintext,
    SmpcAdditiveSharing,
}

impl FederationMode {
    // Converts the federation mode to its stable wire format string.
    // @param: self - Federation mode
    // @return: &'static str - String representation used on the wire
    pub fn as_str(self) -> &'static str {
        match self {
            FederationMode::Plaintext => "plaintext",
            FederationMode::SmpcAdditiveSharing => "smpc_additive_sharing",
        }
    }

    // Returns the full set of supported federation mode strings.
    // @return: &'static [&'static str] - Supported mode names
    pub fn supported() -> &'static [&'static str] {
        &["plaintext", "smpc_additive_sharing"]
    }
}

impl FromStr for FederationMode {
    type Err = anyhow::Error;

    // Parses a federation mode from a user-facing or wire-format string.
    // @param: value - Mode string to parse
    // @return: Result<Self> - Parsed federation mode
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "plaintext" => Ok(FederationMode::Plaintext),
            "smpc_additive_sharing" | "smpc-additive-sharing" => {
                Ok(FederationMode::SmpcAdditiveSharing)
            }
            other => Err(anyhow!("unsupported federation mode '{other}'")),
        }
    }
}
