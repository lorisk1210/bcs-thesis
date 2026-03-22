// src/lib.rs
// Shared protocol surface used by both the hospital node and the orchestrator.

// Protocol modules
pub mod errors;
pub mod federation;
pub mod query;
pub mod stats;

// Re-exports for the most common protocol types.
pub use federation::FederationMode;
pub use query::{ClipBounds, QueryExecutionRequest, QueryResult, QueryTemplate};
pub use stats::{LocalStatistics, aggregate_local_statistics, render_query_result};

// Generated gRPC types compiled from proto/refinery.proto.
pub mod grpc {
    tonic::include_proto!("refinery");
}
