// src/lib.rs
// Shared protocol surface used by both the hospital node and the orchestrator.

// Protocol modules
pub mod dp;
pub mod env_utils;
pub mod errors;
pub mod query;
pub mod release;
pub mod slot_vector;
pub mod smpc;
pub mod stats;

// Re-exports for the most common protocol types.
pub use dp::{apply_noise, count_noised_metrics};
pub use query::{ClipBounds, QueryExecutionRequest, QueryResult, QueryTemplate};
pub use release::{ReleaseMode, release_query_result, release_query_result_with_rng};
pub use slot_vector::{decode_slot_bytes, encode_slot_bytes, sum_slot_vectors};
pub use smpc::{
    PRIVATE_KEY_LENGTH, PUBLIC_KEY_LENGTH, SMPC_AGGREGATE_SHARE_ROUND_NAME, SMPC_PROTOCOL_NAME,
    SMPC_PROTOCOL_VERSION, SharePayload, compute_job_context_hash, decrypt_share_payload,
    encrypt_share_payload, public_key_fingerprint, public_key_from_private_key, sealed_packet_hash,
    slot_vector_hash, split_additive_shares, validate_private_key_bytes,
};
pub use stats::{
    LocalStatistics, StatisticsSchema, aggregate_local_statistics, aggregate_slot_vectors,
    render_query_result, schema_for_query,
};

// Generated gRPC types compiled from proto/refinery.proto.
pub mod grpc {
    tonic::include_proto!("refinery");
}
