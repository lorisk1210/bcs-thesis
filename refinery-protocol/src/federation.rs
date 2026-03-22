use std::str::FromStr;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FederationMode {
    Plaintext,
    SmpcAdditiveSharing,
}

impl FederationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            FederationMode::Plaintext => "plaintext",
            FederationMode::SmpcAdditiveSharing => "smpc_additive_sharing",
        }
    }

    pub fn supported() -> &'static [&'static str] {
        &["plaintext", "smpc_additive_sharing"]
    }
}

impl FromStr for FederationMode {
    type Err = anyhow::Error;

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
