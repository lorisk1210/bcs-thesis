// stats/encoding.rs
// Fixed-point and slot-vector encoding helpers shared across stats modules.

// Third-party library imports
use anyhow::{Result, anyhow};

// Local module imports
use crate::query::QueryTemplate;

const FIXED_POINT_SCALE: f64 = 1_000_000_000.0;
const MAX_SAFE_MAGNITUDE: i64 = i64::MAX / 4;

// Encodes slot values into little-endian bytes.
pub fn encode_slot_bytes(slots: &[u64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(slots.len() * 8);
    for slot in slots {
        bytes.extend_from_slice(&slot.to_le_bytes());
    }
    bytes
}

// Decodes little-endian slot bytes into a slot vector.
pub fn decode_slot_bytes(bytes: &[u8]) -> Result<Vec<u64>> {
    if !bytes.len().is_multiple_of(8) {
        return Err(anyhow!("slot bytes length must be divisible by 8"));
    }

    let mut slots = Vec::with_capacity(bytes.len() / 8);
    for chunk in bytes.chunks_exact(8) {
        let array: [u8; 8] = chunk
            .try_into()
            .map_err(|_| anyhow!("invalid slot byte chunk"))?;
        slots.push(u64::from_le_bytes(array));
    }
    Ok(slots)
}

// Adds multiple slot vectors into one aggregate vector.
pub(crate) fn sum_slot_vectors<'a>(
    template: QueryTemplate,
    slot_count: usize,
    vectors: impl Iterator<Item = &'a [u64]>,
) -> Result<Vec<u64>> {
    let mut slots = vec![0u64; slot_count];
    for vector in vectors {
        if vector.len() != slot_count {
            return Err(anyhow!("slot vector length mismatch for {}", template.as_str()));
        }
        for (index, slot) in vector.iter().enumerate() {
            slots[index] = slots[index].wrapping_add(*slot);
        }
    }
    Ok(slots)
}

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
