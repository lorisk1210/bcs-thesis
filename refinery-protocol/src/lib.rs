pub mod errors;
pub mod federation;
pub mod query;
pub mod stats;

pub use federation::FederationMode;
pub use query::{ClipBounds, QueryExecutionRequest, QueryResult, QueryTemplate};
pub use stats::{LocalStatistics, aggregate_local_statistics, render_query_result};

pub mod grpc {
    tonic::include_proto!("refinery");
}
