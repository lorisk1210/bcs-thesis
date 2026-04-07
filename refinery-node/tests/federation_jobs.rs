use refinery_node::federation::jobs::{
    JOB_STATUS_REJECTED, JOB_STATUS_ROUND1_READY, JobRecord, execute_federation_round,
};
use refinery_node::federation::server::{NodeServerConfig, TlsConfig};
use refinery_node::federation::smpc::SmpcJobState;
use refinery_protocol::SMPC_AGGREGATE_SHARE_ROUND_NAME;
use refinery_protocol::grpc::RunFederationRoundRequest;

fn test_config() -> NodeServerConfig {
    NodeServerConfig {
        node_id: "node-a".to_string(),
        db_path: std::path::PathBuf::from("test.duckdb"),
        input_dir: std::path::PathBuf::from("input"),
        bind_addr: "127.0.0.1:50051".to_string(),
        tls: TlsConfig {
            cert_path: None,
            key_path: None,
            client_ca_cert_path: None,
        },
    }
}

fn test_round_request() -> RunFederationRoundRequest {
    RunFederationRoundRequest {
        job_id: "job".to_string(),
        round_name: SMPC_AGGREGATE_SHARE_ROUND_NAME.to_string(),
        job_context_hash: "hash".to_string(),
        protocol_name: refinery_protocol::SMPC_PROTOCOL_NAME.to_string(),
        protocol_version: refinery_protocol::SMPC_PROTOCOL_VERSION.to_string(),
        schema_id: "schema".to_string(),
        slot_labels: vec!["count".to_string(), "population_in_scope".to_string()],
        share_packets: Vec::new(),
        recipient_node_id: "node-a".to_string(),
    }
}

#[test]
fn round_execution_rejects_unaccepted_jobs() {
    let response = execute_federation_round(
        &test_config(),
        None,
        test_round_request(),
        JobRecord {
            status: JOB_STATUS_REJECTED.to_string(),
            accepted: false,
            reason: "rejected".to_string(),
            smpc_state: None,
        },
    )
    .expect("round execution should return a response");

    assert!(!response.accepted);
    assert_eq!(response.reason, "job is not ready for SMPC round execution");
}

#[test]
fn round_execution_rejects_missing_smpc_state() {
    let response = execute_federation_round(
        &test_config(),
        None,
        test_round_request(),
        JobRecord {
            status: JOB_STATUS_ROUND1_READY.to_string(),
            accepted: true,
            reason: "accepted".to_string(),
            smpc_state: None,
        },
    )
    .expect("round execution should return a response");

    assert!(!response.accepted);
    assert_eq!(response.reason, "job is not ready for SMPC round execution");
}

#[test]
fn round_execution_rejects_missing_capability() {
    let response = execute_federation_round(
        &test_config(),
        None,
        test_round_request(),
        JobRecord {
            status: JOB_STATUS_ROUND1_READY.to_string(),
            accepted: true,
            reason: "accepted".to_string(),
            smpc_state: Some(SmpcJobState {
                job_context_hash: "hash".to_string(),
                schema_id: "schema".to_string(),
                slot_labels: vec!["count".to_string(), "population_in_scope".to_string()],
                protocol_name: refinery_protocol::SMPC_PROTOCOL_NAME.to_string(),
                protocol_version: refinery_protocol::SMPC_PROTOCOL_VERSION.to_string(),
                participant_keys: Default::default(),
            }),
        },
    )
    .expect("round execution should return a response");

    assert!(!response.accepted);
    assert_eq!(
        response.reason,
        "SMPC capability is not configured on this node"
    );
}
