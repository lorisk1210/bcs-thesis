// src/smpc.rs
// Shared helpers for orchestrator-relayed additive secret sharing.

// Third-party library imports
use anyhow::{Result, anyhow};
use crypto_box::{
    ChaChaBox, PublicKey, SecretKey,
    aead::{Aead, AeadCore, OsRng, generic_array::GenericArray, rand_core::RngCore},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// Local module imports
use crate::grpc::{ParticipantManifestEntry, SealedSharePacket};

pub const SMPC_PROTOCOL_NAME: &str = "smpc_additive_sharing";
pub const SMPC_PROTOCOL_VERSION: &str = "v1";
pub const SMPC_AGGREGATE_SHARE_ROUND_NAME: &str = "aggregate_share_v1";
pub const PUBLIC_KEY_LENGTH: usize = 32;
pub const PRIVATE_KEY_LENGTH: usize = 32;

// Encrypted payload delivered from one node to another via the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SharePayload {
    pub job_id: String,
    pub job_context_hash: String,
    pub protocol_name: String,
    pub protocol_version: String,
    pub sender_node_id: String,
    pub recipient_node_id: String,
    pub schema_id: String,
    pub slot_labels: Vec<String>,
    pub slot_bytes: Vec<u8>,
}

// Parses a raw 32-byte SMPC private key.
pub fn validate_private_key_bytes(bytes: &[u8]) -> Result<[u8; PRIVATE_KEY_LENGTH]> {
    bytes
        .try_into()
        .map_err(|_| anyhow!("SMPC private key must be 32 bytes"))
}

// Derives the public key for a raw 32-byte SMPC private key.
pub fn public_key_from_private_key(private_key: &[u8; PRIVATE_KEY_LENGTH]) -> Vec<u8> {
    let secret = SecretKey::from(*private_key);
    secret.public_key().as_bytes().to_vec()
}

// Computes a stable fingerprint for one SMPC public key.
pub fn public_key_fingerprint(public_key: &[u8]) -> String {
    let digest = Sha256::digest(public_key);
    hex::encode(digest)
}

// Computes a stable job-context hash that binds query and participant metadata.
pub fn compute_job_context_hash(
    job_id: &str,
    template: &str,
    params_json: &str,
    clip_min: f64,
    clip_max: f64,
    protocol_name: &str,
    protocol_version: &str,
    participants: &[ParticipantManifestEntry],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(job_id.as_bytes());
    hasher.update(template.as_bytes());
    hasher.update(params_json.as_bytes());
    hasher.update(clip_min.to_le_bytes());
    hasher.update(clip_max.to_le_bytes());
    hasher.update(protocol_name.as_bytes());
    hasher.update(protocol_version.as_bytes());

    let mut participant_rows = participants
        .iter()
        .map(|participant| {
            format!(
                "{}|{}|{}",
                participant.node_id, participant.endpoint, participant.smpc_key_fingerprint
            )
        })
        .collect::<Vec<_>>();
    participant_rows.sort();
    for row in participant_rows {
        hasher.update(row.as_bytes());
    }

    hex::encode(hasher.finalize())
}

// Splits one canonical vector into additive shares over the 64-bit ring.
pub fn split_additive_shares(slots: &[u64], share_count: usize) -> Result<Vec<Vec<u64>>> {
    if share_count < 2 {
        return Err(anyhow!("at least two shares are required"));
    }

    let mut shares = vec![vec![0u64; slots.len()]; share_count];
    let mut rng = OsRng;

    for (slot_index, value) in slots.iter().enumerate() {
        let mut sum = 0u64;
        for share_index in 0..(share_count - 1) {
            let random = rng.next_u64();
            shares[share_index][slot_index] = random;
            sum = sum.wrapping_add(random);
        }
        shares[share_count - 1][slot_index] = value.wrapping_sub(sum);
    }

    Ok(shares)
}

// Encrypts one share payload for a specific recipient.
pub fn encrypt_share_payload(
    sender_private_key: &[u8; PRIVATE_KEY_LENGTH],
    recipient_public_key: &[u8],
    payload: &SharePayload,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let sender_secret = SecretKey::from(*sender_private_key);
    let recipient_public = parse_public_key(recipient_public_key)?;
    let cipher = ChaChaBox::new(&recipient_public, &sender_secret);
    let nonce = ChaChaBox::generate_nonce(&mut OsRng);
    let plaintext = serde_json::to_vec(payload)?;
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|_| anyhow!("failed to encrypt share payload"))?;
    Ok((nonce.to_vec(), ciphertext))
}

// Decrypts one inbound share payload for the recipient node.
pub fn decrypt_share_payload(
    recipient_private_key: &[u8; PRIVATE_KEY_LENGTH],
    sender_public_key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<SharePayload> {
    let recipient_secret = SecretKey::from(*recipient_private_key);
    let sender_public = parse_public_key(sender_public_key)?;
    let cipher = ChaChaBox::new(&sender_public, &recipient_secret);
    let nonce = GenericArray::clone_from_slice(nonce);
    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| anyhow!("failed to decrypt share payload"))?;
    Ok(serde_json::from_slice(&plaintext)?)
}

// Computes a stable integrity hash for one share packet.
pub fn sealed_packet_hash(packet: &SealedSharePacket) -> String {
    let mut hasher = Sha256::new();
    hasher.update(packet.job_id.as_bytes());
    hasher.update(packet.job_context_hash.as_bytes());
    hasher.update(packet.protocol_name.as_bytes());
    hasher.update(packet.protocol_version.as_bytes());
    hasher.update(packet.sender_node_id.as_bytes());
    hasher.update(packet.recipient_node_id.as_bytes());
    hasher.update(packet.schema_id.as_bytes());
    for label in &packet.slot_labels {
        hasher.update(label.as_bytes());
    }
    hasher.update(&packet.nonce);
    hasher.update(&packet.ciphertext);
    hex::encode(hasher.finalize())
}

// Computes a stable integrity hash for one encoded slot vector.
pub fn slot_vector_hash(slot_bytes: &[u8]) -> String {
    let digest = Sha256::digest(slot_bytes);
    hex::encode(digest)
}

fn parse_public_key(bytes: &[u8]) -> Result<PublicKey> {
    let key_bytes: [u8; PUBLIC_KEY_LENGTH] = bytes
        .try_into()
        .map_err(|_| anyhow!("SMPC public key must be 32 bytes"))?;
    Ok(PublicKey::from(key_bytes))
}
