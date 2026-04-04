use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};
use refinery_protocol::grpc::{
    RunFederationRoundRequest, RunFederationRoundResponse, SealedSharePacket, SubmitJobRequest,
};
use refinery_protocol::{
    LocalStatistics, PRIVATE_KEY_LENGTH, SMPC_AGGREGATE_SHARE_ROUND_NAME, SMPC_PROTOCOL_NAME,
    SMPC_PROTOCOL_VERSION, SharePayload, decode_slot_bytes, encode_slot_bytes,
    encrypt_share_payload, public_key_fingerprint, public_key_from_private_key, sealed_packet_hash,
    slot_vector_hash, split_additive_shares, sum_slot_vectors,
};
use zeroize::Zeroize;

#[derive(Debug, Clone)]
pub struct SmpcCapability {
    pub private_key_bytes: [u8; PRIVATE_KEY_LENGTH],
    pub public_key: Vec<u8>,
    pub fingerprint: String,
    pub min_participating_nodes: usize,
}

#[derive(Debug, Clone)]
pub struct SmpcJobState {
    pub job_context_hash: String,
    pub schema_id: String,
    pub slot_labels: Vec<String>,
    pub protocol_name: String,
    pub protocol_version: String,
    pub participant_keys: BTreeMap<String, Vec<u8>>,
}

pub fn load_smpc_capability() -> Result<Option<SmpcCapability>> {
    let config = crate::config::load_smpc_config()?;
    let Some(private_key_bytes) = config.private_key_bytes else {
        return Ok(None);
    };
    let public_key = public_key_from_private_key(&private_key_bytes);
    let fingerprint = public_key_fingerprint(&public_key);
    Ok(Some(SmpcCapability {
        private_key_bytes,
        public_key,
        fingerprint,
        min_participating_nodes: config.min_participating_nodes,
    }))
}

pub fn smpc_override_rejection_reason(
    request: &SubmitJobRequest,
    node_id: &str,
    smpc_capability: Option<&SmpcCapability>,
) -> Option<String> {
    let Some(smpc) = smpc_capability else {
        return Some("SMPC capability is not configured on this node".to_string());
    };
    if request.protocol_name != SMPC_PROTOCOL_NAME
        || request.protocol_version != SMPC_PROTOCOL_VERSION
    {
        return Some("unsupported SMPC protocol metadata".to_string());
    }
    if request.participants.len() < smpc.min_participating_nodes {
        return Some(format!(
            "SMPC mode requires at least {} participating nodes",
            smpc.min_participating_nodes
        ));
    }
    let Some(own_manifest) = request
        .participants
        .iter()
        .find(|participant| participant.node_id == node_id)
    else {
        return Some("participant manifest does not include this node".to_string());
    };
    if own_manifest.smpc_public_key != smpc.public_key
        || own_manifest.smpc_key_fingerprint != smpc.fingerprint
    {
        return Some("participant manifest SMPC key mismatch".to_string());
    }
    None
}

pub fn build_share_packets(
    request: &SubmitJobRequest,
    node_id: &str,
    smpc: &SmpcCapability,
    stats: &LocalStatistics,
) -> Result<Vec<SealedSharePacket>> {
    let share_vectors = split_additive_shares(&stats.slots, request.participants.len())?;
    let mut share_packets = Vec::with_capacity(request.participants.len());

    for (participant, share_vector) in request.participants.iter().zip(share_vectors) {
        let payload = SharePayload {
            job_id: request.job_id.clone(),
            job_context_hash: request.job_context_hash.clone(),
            protocol_name: request.protocol_name.clone(),
            protocol_version: request.protocol_version.clone(),
            sender_node_id: node_id.to_string(),
            recipient_node_id: participant.node_id.clone(),
            schema_id: stats.schema_id.clone(),
            slot_labels: stats.slot_labels.clone(),
            slot_bytes: encode_slot_bytes(&share_vector),
        };
        let (nonce, ciphertext) = encrypt_share_payload(
            &smpc.private_key_bytes,
            &participant.smpc_public_key,
            &payload,
        )?;
        let mut packet = SealedSharePacket {
            job_id: request.job_id.clone(),
            job_context_hash: request.job_context_hash.clone(),
            protocol_name: request.protocol_name.clone(),
            protocol_version: request.protocol_version.clone(),
            sender_node_id: node_id.to_string(),
            recipient_node_id: participant.node_id.clone(),
            schema_id: stats.schema_id.clone(),
            slot_labels: stats.slot_labels.clone(),
            nonce,
            ciphertext,
            packet_hash: String::new(),
        };
        packet.packet_hash = sealed_packet_hash(&packet);
        share_packets.push(packet);
    }

    Ok(share_packets)
}

pub fn validate_round_request(
    request: &RunFederationRoundRequest,
    state: &SmpcJobState,
    node_id: &str,
) -> Option<String> {
    if request.round_name != SMPC_AGGREGATE_SHARE_ROUND_NAME {
        return Some("unsupported SMPC round name".to_string());
    }
    if request.recipient_node_id != node_id {
        return Some("round recipient does not match node id".to_string());
    }
    if request.job_context_hash != state.job_context_hash {
        return Some("job context hash mismatch".to_string());
    }
    if request.protocol_name != state.protocol_name
        || request.protocol_version != state.protocol_version
    {
        return Some("SMPC protocol metadata mismatch".to_string());
    }
    if request.schema_id != state.schema_id || request.slot_labels != state.slot_labels {
        return Some("statistics schema mismatch".to_string());
    }
    if request.share_packets.len() != state.participant_keys.len() {
        return Some("unexpected number of inbound share packets".to_string());
    }
    let mut seen_senders = BTreeSet::new();
    for packet in &request.share_packets {
        if !state.participant_keys.contains_key(&packet.sender_node_id) {
            return Some("sender node is not part of the approved manifest".to_string());
        }
        if !seen_senders.insert(packet.sender_node_id.clone()) {
            return Some("duplicate share packet sender".to_string());
        }
    }
    None
}

pub fn aggregate_inbound_share_packets(
    request: &RunFederationRoundRequest,
    state: &SmpcJobState,
    node_id: &str,
    capability: &SmpcCapability,
) -> Result<(Vec<u8>, String)> {
    let mut inbound_vectors = Vec::with_capacity(request.share_packets.len());

    for packet in &request.share_packets {
        if let Some(reason) = validate_share_packet(packet, request, node_id) {
            return Err(anyhow!(reason));
        }
        if packet.packet_hash != sealed_packet_hash(packet) {
            return Err(anyhow!("share packet hash mismatch"));
        }
        let Some(sender_public_key) = state.participant_keys.get(&packet.sender_node_id) else {
            return Err(anyhow!("sender node is not part of the approved manifest"));
        };
        let mut payload = refinery_protocol::decrypt_share_payload(
            &capability.private_key_bytes,
            sender_public_key,
            &packet.nonce,
            &packet.ciphertext,
        )?;
        if let Some(reason) = validate_share_payload(&payload, packet, request) {
            payload.slot_bytes.zeroize();
            return Err(anyhow!(reason));
        }
        let slots = decode_slot_bytes(&payload.slot_bytes)?;
        payload.slot_bytes.zeroize();
        inbound_vectors.push(slots);
    }

    let mut aggregate_share = sum_slot_vectors(&inbound_vectors)?;
    for vector in &mut inbound_vectors {
        vector.zeroize();
    }
    let aggregate_share_bytes = encode_slot_bytes(&aggregate_share);
    let vector_hash = slot_vector_hash(&aggregate_share_bytes);
    aggregate_share.zeroize();
    Ok((aggregate_share_bytes, vector_hash))
}

pub fn rejected_round_response(
    node_id: &str,
    request: &RunFederationRoundRequest,
    reason: &str,
) -> RunFederationRoundResponse {
    RunFederationRoundResponse {
        accepted: false,
        reason: reason.to_string(),
        node_id: node_id.to_string(),
        schema_id: request.schema_id.clone(),
        slot_labels: request.slot_labels.clone(),
        aggregate_share: Vec::new(),
        vector_hash: String::new(),
        job_context_hash: request.job_context_hash.clone(),
        protocol_name: request.protocol_name.clone(),
        protocol_version: request.protocol_version.clone(),
    }
}

fn validate_share_packet(
    packet: &SealedSharePacket,
    request: &RunFederationRoundRequest,
    node_id: &str,
) -> Option<String> {
    if let Some(reason) = validate_share_context(
        "packet",
        &packet.job_id,
        &packet.job_context_hash,
        &packet.protocol_name,
        &packet.protocol_version,
        &packet.schema_id,
        &packet.slot_labels,
        request,
    ) {
        return Some(reason);
    }
    if packet.recipient_node_id != node_id {
        return Some("share packet recipient mismatch".to_string());
    }
    None
}

fn validate_share_payload(
    payload: &SharePayload,
    packet: &SealedSharePacket,
    request: &RunFederationRoundRequest,
) -> Option<String> {
    if let Some(reason) = validate_share_context(
        "payload",
        &payload.job_id,
        &payload.job_context_hash,
        &payload.protocol_name,
        &payload.protocol_version,
        &payload.schema_id,
        &payload.slot_labels,
        request,
    ) {
        return Some(reason);
    }
    if payload.sender_node_id != packet.sender_node_id
        || payload.recipient_node_id != packet.recipient_node_id
    {
        return Some("share payload sender or recipient mismatch".to_string());
    }
    None
}

fn validate_share_context(
    label: &str,
    job_id: &str,
    job_context_hash: &str,
    protocol_name: &str,
    protocol_version: &str,
    schema_id: &str,
    slot_labels: &[String],
    request: &RunFederationRoundRequest,
) -> Option<String> {
    if job_id != request.job_id {
        return Some(format!("share {label} job id mismatch"));
    }
    if job_context_hash != request.job_context_hash {
        return Some(format!("share {label} context hash mismatch"));
    }
    if protocol_name != request.protocol_name || protocol_version != request.protocol_version {
        return Some(format!("share {label} protocol mismatch"));
    }
    if schema_id != request.schema_id || slot_labels != request.slot_labels {
        return Some(format!("share {label} schema mismatch"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use refinery_protocol::QueryTemplate;
    use refinery_protocol::grpc::ParticipantManifestEntry;

    #[test]
    fn smpc_override_rejection_requires_minimum_participants() {
        let capability = SmpcCapability {
            private_key_bytes: [7u8; 32],
            public_key: vec![1u8; 32],
            fingerprint: "fingerprint".to_string(),
            min_participating_nodes: 3,
        };
        let request = SubmitJobRequest {
            job_id: "job".to_string(),
            template: QueryTemplate::CohortFeasibilityCount.as_str().to_string(),
            params_json: "{}".to_string(),
            clip_min: 0.0,
            clip_max: 300.0,
            protocol_name: SMPC_PROTOCOL_NAME.to_string(),
            protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            job_context_hash: "hash".to_string(),
            participants: vec![
                ParticipantManifestEntry {
                    node_id: "node-a".to_string(),
                    endpoint: "http://127.0.0.1:1".to_string(),
                    smpc_public_key: capability.public_key.clone(),
                    smpc_key_fingerprint: capability.fingerprint.clone(),
                },
                ParticipantManifestEntry {
                    node_id: "node-b".to_string(),
                    endpoint: "http://127.0.0.1:2".to_string(),
                    smpc_public_key: vec![2u8; 32],
                    smpc_key_fingerprint: "other".to_string(),
                },
            ],
        };

        let reason = smpc_override_rejection_reason(&request, "node-a", Some(&capability))
            .expect("request should be rejected");
        assert!(reason.contains("at least 3 participating nodes"));
    }

    #[test]
    fn rejected_round_response_drops_share_bytes() {
        let request = RunFederationRoundRequest {
            job_id: "job".to_string(),
            round_name: SMPC_AGGREGATE_SHARE_ROUND_NAME.to_string(),
            job_context_hash: "hash".to_string(),
            protocol_name: SMPC_PROTOCOL_NAME.to_string(),
            protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            schema_id: "schema".to_string(),
            slot_labels: vec!["count".to_string(), "population_in_scope".to_string()],
            share_packets: Vec::new(),
            recipient_node_id: "node-a".to_string(),
        };

        let response = rejected_round_response("node-a", &request, "bad packet");
        assert!(!response.accepted);
        assert!(response.aggregate_share.is_empty());
        assert_eq!(response.reason, "bad packet");
    }

    #[test]
    fn validate_round_request_rejects_duplicate_senders() {
        let request = RunFederationRoundRequest {
            job_id: "job".to_string(),
            round_name: SMPC_AGGREGATE_SHARE_ROUND_NAME.to_string(),
            job_context_hash: "hash".to_string(),
            protocol_name: SMPC_PROTOCOL_NAME.to_string(),
            protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            schema_id: "schema".to_string(),
            slot_labels: vec!["count".to_string(), "population_in_scope".to_string()],
            share_packets: vec![
                SealedSharePacket {
                    job_id: "job".to_string(),
                    job_context_hash: "hash".to_string(),
                    protocol_name: SMPC_PROTOCOL_NAME.to_string(),
                    protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
                    sender_node_id: "node-a".to_string(),
                    recipient_node_id: "node-b".to_string(),
                    schema_id: "schema".to_string(),
                    slot_labels: vec!["count".to_string(), "population_in_scope".to_string()],
                    nonce: vec![1],
                    ciphertext: vec![2],
                    packet_hash: "hash-a".to_string(),
                },
                SealedSharePacket {
                    job_id: "job".to_string(),
                    job_context_hash: "hash".to_string(),
                    protocol_name: SMPC_PROTOCOL_NAME.to_string(),
                    protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
                    sender_node_id: "node-a".to_string(),
                    recipient_node_id: "node-b".to_string(),
                    schema_id: "schema".to_string(),
                    slot_labels: vec!["count".to_string(), "population_in_scope".to_string()],
                    nonce: vec![3],
                    ciphertext: vec![4],
                    packet_hash: "hash-b".to_string(),
                },
            ],
            recipient_node_id: "node-b".to_string(),
        };
        let state = SmpcJobState {
            job_context_hash: "hash".to_string(),
            schema_id: "schema".to_string(),
            slot_labels: vec!["count".to_string(), "population_in_scope".to_string()],
            protocol_name: SMPC_PROTOCOL_NAME.to_string(),
            protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            participant_keys: BTreeMap::from([
                ("node-a".to_string(), vec![1u8; 32]),
                ("node-b".to_string(), vec![2u8; 32]),
            ]),
        };

        let reason =
            validate_round_request(&request, &state, "node-b").expect("request should reject");
        assert_eq!(reason, "duplicate share packet sender");
    }
}
