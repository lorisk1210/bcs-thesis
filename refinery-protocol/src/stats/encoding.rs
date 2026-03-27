// stats/encoding.rs
// Fixed-point encoding helpers shared across stats modules.

// Third-party library imports
use anyhow::{Result, anyhow};

const FIXED_POINT_SCALE: f64 = 1_000_000_000.0;
const MAX_SAFE_MAGNITUDE: i64 = i64::MAX / 4;

pub(crate) fn encode_count(value: u64) -> Result<u64> {
    let value = i64::try_from(value).map_err(|_| anyhow!("count exceeds supported range"))?;
    encode_signed(value)
}

pub(crate) fn decode_count(slot: u64) -> Result<u64> {
    let value = decode_signed(slot)?;
    if value < 0 {
        return Err(anyhow!("decoded count is negative"));
    }
    Ok(value as u64)
}

pub(crate) fn encode_fixed(value: f64) -> Result<u64> {
    if !value.is_finite() {
        return Err(anyhow!("fixed-point value must be finite"));
    }
    let scaled = (value * FIXED_POINT_SCALE).round();
    if scaled.abs() > MAX_SAFE_MAGNITUDE as f64 {
        return Err(anyhow!("fixed-point value exceeds supported range"));
    }
    encode_signed(scaled as i64)
}

pub(crate) fn decode_fixed(slot: u64) -> Result<f64> {
    Ok(decode_signed(slot)? as f64 / FIXED_POINT_SCALE)
}

pub(crate) fn encode_signed(value: i64) -> Result<u64> {
    if value.abs() > MAX_SAFE_MAGNITUDE {
        return Err(anyhow!("signed value exceeds supported range"));
    }
    Ok(value as u64)
}

pub(crate) fn decode_signed(slot: u64) -> Result<i64> {
    let value = slot as i64;
    if value.abs() > MAX_SAFE_MAGNITUDE {
        return Err(anyhow!("decoded value exceeds supported range"));
    }
    Ok(value)
}
