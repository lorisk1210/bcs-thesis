// Shared run output for both plaintext and SMPC federation paths.

use refinery_protocol::QueryResult;

// Final output of one federated execution after transport and aggregation.
#[derive(Debug, Clone)]
pub struct FederatedRunOutput {
    pub aggregated: QueryResult,
    pub accepted_nodes: usize,
    pub job_context_hash: Option<String>,
}
