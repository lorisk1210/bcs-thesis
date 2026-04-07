use std::collections::BTreeMap;

use refinery_node::federation::smpc::{
    SmpcCapability, SmpcJobState, rejected_round_response, smpc_override_rejection_reason,
    validate_round_request,
};
use refinery_protocol::QueryTemplate;
use refinery_protocol::grpc::{
    ParticipantManifestEntry, RunFederationRoundRequest, SealedSharePacket, SubmitJobRequest,
};
use refinery_protocol::{
    SMPC_AGGREGATE_SHARE_ROUND_NAME, SMPC_PROTOCOL_NAME, SMPC_PROTOCOL_VERSION,
};

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

    let reason =
        smpc_override_rejection_reason(&request, "node-a", Some(&capability)).expect("reject");
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

    let reason = validate_round_request(&request, &state, "node-b").expect("request should reject");
    assert_eq!(reason, "duplicate share packet sender");
}
