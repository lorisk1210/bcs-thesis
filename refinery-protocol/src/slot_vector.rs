// src/slot_vector.rs
// Shared slot-vector transport and ring-arithmetic helpers.

// Third-party library imports
use anyhow::{Result, anyhow};

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

// Adds multiple equal-length slot vectors inside the same 64-bit ring.
pub fn sum_slot_vectors(vectors: &[Vec<u64>]) -> Result<Vec<u64>> {
    let Some(first) = vectors.first() else {
        return Err(anyhow!("cannot sum zero slot vectors"));
    };
    sum_slot_slices(first.len(), vectors.iter().map(|vector| vector.as_slice()))
}

// Adds multiple equal-length slot slices inside the same 64-bit ring.
pub(crate) fn sum_slot_slices<'a>(
    slot_count: usize,
    vectors: impl Iterator<Item = &'a [u64]>,
) -> Result<Vec<u64>> {
    let mut slots = vec![0u64; slot_count];
    for vector in vectors {
        if vector.len() != slot_count {
            return Err(anyhow!("slot vector length mismatch"));
        }
        for (index, slot) in vector.iter().enumerate() {
            slots[index] = slots[index].wrapping_add(*slot);
        }
    }
    Ok(slots)
}
