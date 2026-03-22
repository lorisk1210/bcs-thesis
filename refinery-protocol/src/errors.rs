// src/errors.rs
// Shared protocol error helpers.

// Third-party library imports
use anyhow::anyhow;

// Builds a standardized error for malformed local statistics payloads.
// @param: context - Name of the field or structure that failed validation
// @return: anyhow::Error - Error describing the invalid statistics shape
pub fn invalid_stats_shape(context: &str) -> anyhow::Error {
    anyhow!("invalid local statistics shape for {context}")
}
